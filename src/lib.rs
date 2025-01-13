// Copyright 2016-2020 mime-multipart Developers
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

pub mod error;

#[cfg(test)]
mod tests;

pub use error::Error;

use buf_read_ext::BufReadExt;
use http::header::{HeaderMap, HeaderName, HeaderValue};
use mime::Mime;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::ops::Drop;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use textnonce::TextNonce;

/// A multipart part which is not a file (stored in memory)
#[derive(Clone, Debug, PartialEq)]
pub struct Part {
    pub headers: HeaderMap,
    pub body: Vec<u8>,
}
impl Part {
    /// Mime content-type specified in the header
    pub fn content_type(&self) -> Option<Mime> {
        match self.headers.get("content-type") {
            Some(ct) => match ct.to_str() {
                Ok(value) => match Mime::from_str(value) {
                    Ok(value) => Some(value),
                    Err(_) => None,
                },
                Err(_) => None,
            },
            None => None,
        }
    }
}

/// A file that is to be inserted into a `multipart/*` or alternatively an uploaded file that
/// was received as part of `multipart/*` parsing.
#[derive(Clone, Debug, PartialEq)]
pub struct FilePart {
    /// The headers of the part
    pub headers: HeaderMap,
    /// A temporary file containing the file content
    pub path: PathBuf,
    /// Optionally, the size of the file.  This is filled when multiparts are parsed, but is
    /// not necessary when they are generated.
    pub size: Option<usize>,
    // The temporary directory the upload was put into, saved for the Drop trait
    tempdir: Option<PathBuf>,
}
impl FilePart {
    pub fn new(headers: HeaderMap, path: &Path) -> FilePart {
        FilePart {
            headers,
            path: path.to_owned(),
            size: None,
            tempdir: None,
        }
    }

    /// If you do not want the file on disk to be deleted when Self drops, call this
    /// function.  It will become your responsability to clean up.
    pub fn do_not_delete_on_drop(&mut self) {
        self.tempdir = None;
    }

    /// Create a new temporary FilePart (when created this way, the file will be
    /// deleted once the FilePart object goes out of scope).
    pub fn create(headers: HeaderMap) -> Result<FilePart, Error> {
        // Setup a file to capture the contents.
        let mut path = tempfile::Builder::new()
            .prefix("mime_multipart")
            .tempdir()?
            .into_path();
        let tempdir = Some(path.clone());
        path.push(TextNonce::sized_urlsafe(32).unwrap().into_string());
        Ok(FilePart {
            headers,
            path,
            size: None,
            tempdir,
        })
    }

    /// Filename that was specified when the file was uploaded.  Returns `Ok<None>` if there
    /// was no content-disposition header supplied.
    pub fn filename(&self) -> Result<Option<String>, Error> {
        match self.headers.get("content-disposition") {
            Some(cd) => get_content_disposition_filename(cd),
            None => Ok(None),
        }
    }

    /// Mime content-type specified in the header
    pub fn content_type(&self) -> Option<Mime> {
        match self.headers.get("content-type") {
            Some(ct) => match ct.to_str() {
                Ok(value) => match Mime::from_str(value) {
                    Ok(value) => Some(value),
                    Err(_) => None,
                },
                Err(_) => None,
            },
            None => None,
        }
    }
}
impl Drop for FilePart {
    fn drop(&mut self) {
        if self.tempdir.is_some() {
            let _ = std::fs::remove_file(&self.path);
            let _ = std::fs::remove_dir(self.tempdir.as_ref().unwrap());
        }
    }
}

/// A multipart part which could be either a file, in memory, or another multipart
/// container containing nested parts.
#[derive(Clone, Debug)]
pub enum Node {
    /// A part in memory
    Part(Part),
    /// A part streamed to a file
    File(FilePart),
    /// A container of nested multipart parts
    Multipart((HeaderMap, Vec<Node>)),
}

/// Parse a MIME `multipart/*` from a `Read`able stream into a `Vec` of `Node`s, streaming
/// files to disk and keeping the rest in memory.  Recursive `multipart/*` parts will are
/// parsed as well and returned within a `Node::Multipart` variant.
///
/// If `always_use_files` is true, all parts will be streamed to files.  If false, only parts
/// with a `ContentDisposition` header set to `Attachment` or otherwise containing a `Filename`
/// parameter will be streamed to files.
///
/// It is presumed that the headers are still in the stream.  If you have them separately,
/// use `read_multipart_body()` instead.
pub fn read_multipart<S: Read>(stream: &mut S, always_use_files: bool) -> Result<Vec<Node>, Error> {
    let mut reader = BufReader::with_capacity(4096, stream);
    let mut nodes: Vec<Node> = Vec::new();

    let mut buf: Vec<u8> = Vec::new();

    let (_, found) = reader.stream_until_token(b"\r\n\r\n", &mut buf)?;
    if !found {
        return Err(Error::EofInMainHeaders);
    }

    // Keep the CRLFCRLF as httparse will expect it
    buf.extend(b"\r\n\r\n".iter().cloned());

    // Parse the headers
    let mut header_memory = [httparse::EMPTY_HEADER; 64];
    let headers = match httparse::parse_headers(&buf, &mut header_memory) {
        Ok(httparse::Status::Complete((_, raw_headers))) => {
            let mut headers = HeaderMap::new();
            for header in raw_headers {
                if header.value.is_empty() {
                    break;
                }
                let trim = header
                    .value
                    .iter()
                    .rev()
                    .take_while(|&&x| x == b' ')
                    .count();
                let value = &header.value[..header.value.len() - trim];

                let header_value = match HeaderValue::from_bytes(value) {
                    Ok(value) => value,
                    Err(_) => return Err(Error::InvalidHeaderNameOrValue),
                };

                let header_name = header.name.to_owned();
                let header_name = match HeaderName::from_str(&header_name) {
                    Ok(value) => value,
                    Err(_) => return Err(Error::InvalidHeaderNameOrValue),
                };
                headers.append(header_name, header_value);
            }
            Ok(headers)
        }
        Ok(httparse::Status::Partial) => Err(Error::PartialHeaders),
        Err(err) => Err(From::from(err)),
    }?;

    inner(&mut reader, &headers, &mut nodes, always_use_files)?;
    Ok(nodes)
}

/// Parse a MIME `multipart/*` from a `Read`able stream into a `Vec` of `Node`s, streaming
/// files to disk and keeping the rest in memory.  Recursive `multipart/*` parts will are
/// parsed as well and returned within a `Node::Multipart` variant.
///
/// If `always_use_files` is true, all parts will be streamed to files.  If false, only parts
/// with a `ContentDisposition` header set to `Attachment` or otherwise containing a `Filename`
/// parameter will be streamed to files.
///
/// It is presumed that you have the `Headers` already and the stream starts at the body.
/// If the headers are still in the stream, use `read_multipart()` instead.
pub fn read_multipart_body<S: Read>(
    stream: &mut S,
    headers: &HeaderMap,
    always_use_files: bool,
) -> Result<Vec<Node>, Error> {
    let mut reader = BufReader::with_capacity(4096, stream);
    let mut nodes: Vec<Node> = Vec::new();
    inner(&mut reader, headers, &mut nodes, always_use_files)?;
    Ok(nodes)
}

fn inner<R: BufRead>(
    reader: &mut R,
    headers: &HeaderMap,
    nodes: &mut Vec<Node>,
    always_use_files: bool,
) -> Result<(), Error> {
    let mut buf: Vec<u8> = Vec::new();

    let boundary = get_multipart_boundary(headers)?;

    // Read past the initial boundary
    let (_, found) = reader.stream_until_token(&boundary, &mut buf)?;
    if !found {
        return Err(Error::EofBeforeFirstBoundary);
    }

    // Define the boundary, including the line terminator preceding it.
    // Use their first line terminator to determine whether to use CRLF or LF.
    let (lt, ltlt, lt_boundary) = {
        let peeker = reader.fill_buf()?;
        if peeker.len() > 1 && &peeker[..2] == b"\r\n" {
            let mut output = Vec::with_capacity(2 + boundary.len());
            output.push(b'\r');
            output.push(b'\n');
            output.extend(boundary.clone());
            (vec![b'\r', b'\n'], vec![b'\r', b'\n', b'\r', b'\n'], output)
        } else if !peeker.is_empty() && peeker[0] == b'\n' {
            let mut output = Vec::with_capacity(1 + boundary.len());
            output.push(b'\n');
            output.extend(boundary.clone());
            (vec![b'\n'], vec![b'\n', b'\n'], output)
        } else {
            return Err(Error::NoCrLfAfterBoundary);
        }
    };

    loop {
        // If the next two lookahead characters are '--', parsing is finished.
        {
            let peeker = reader.fill_buf()?;
            if peeker.len() >= 2 && &peeker[..2] == b"--" {
                return Ok(());
            }
        }

        // Read the line terminator after the boundary
        let (_, found) = reader.stream_until_token(&lt, &mut buf)?;
        if !found {
            return Err(Error::NoCrLfAfterBoundary);
        }

        // Read the headers (which end in 2 line terminators)
        buf.truncate(0); // start fresh
        let (_, found) = reader.stream_until_token(&ltlt, &mut buf)?;
        if !found {
            return Err(Error::EofInPartHeaders);
        }

        // Keep the 2 line terminators as httparse will expect it
        buf.extend(ltlt.iter().cloned());

        // Parse the headers
        let part_headers = {
            let mut header_memory = [httparse::EMPTY_HEADER; 4];
            match httparse::parse_headers(&buf, &mut header_memory) {
                Ok(httparse::Status::Complete((_, raw_headers))) => {
                    let mut headers = HeaderMap::new();
                    for header in raw_headers {
                        if header.value.is_empty() {
                            break;
                        }
                        let trim = header
                            .value
                            .iter()
                            .rev()
                            .take_while(|&&x| x == b' ')
                            .count();
                        let value = &header.value[..header.value.len() - trim];

                        let header_value = match HeaderValue::from_bytes(value) {
                            Ok(value) => value,
                            Err(_) => return Err(Error::InvalidHeaderNameOrValue),
                        };

                        let header_name = header.name.to_owned();
                        let header_name = match HeaderName::from_str(&header_name) {
                            Ok(value) => value,
                            Err(_) => return Err(Error::InvalidHeaderNameOrValue),
                        };
                        headers.append(header_name, header_value);
                    }
                    Ok(headers)
                }
                Ok(httparse::Status::Partial) => Err(Error::PartialHeaders),
                Err(err) => Err(From::from(err)),
            }?
        };

        // Check for a nested multipart
        let nested = {
            match part_headers.get("content-type") {
                Some(ct) => match ct.to_str() {
                    Ok(value) => match Mime::from_str(value) {
                        Ok(mime) => mime.type_() == mime::MULTIPART,
                        Err(_) => return Err(Error::HeaderValueNotMime),
                    },
                    Err(err) => return Err(Error::ToStr(err)),
                },
                None => false,
            }
        };
        if nested {
            // Recurse:
            let mut inner_nodes: Vec<Node> = Vec::new();
            inner(reader, &part_headers, &mut inner_nodes, always_use_files)?;
            nodes.push(Node::Multipart((part_headers, inner_nodes)));
            continue;
        }

        let is_file = always_use_files || {
            match part_headers.get("content-disposition") {
                Some(content) => match content.to_str() {
                    Ok(value) => value.contains("attachment") || value.contains("filename"),
                    Err(err) => return Err(Error::ToStr(err)),
                },
                None => false,
            }
        };
        if is_file {
            // Setup a file to capture the contents.
            let mut filepart = FilePart::create(part_headers)?;
            let mut file = File::create(filepart.path.clone())?;

            // Stream out the file.
            let (read, found) = reader.stream_until_token(&lt_boundary, &mut file)?;
            if !found {
                return Err(Error::EofInFile);
            }
            filepart.size = Some(read);

            // TODO: Handle Content-Transfer-Encoding.  RFC 7578 section 4.7 deprecated
            // this, and the authors state "Currently, no deployed implementations that
            // send such bodies have been discovered", so this is very low priority.

            nodes.push(Node::File(filepart));
        } else {
            buf.truncate(0); // start fresh
            let (_, found) = reader.stream_until_token(&lt_boundary, &mut buf)?;
            if !found {
                return Err(Error::EofInPart);
            }

            nodes.push(Node::Part(Part {
                headers: part_headers,
                body: buf.clone(),
            }));
        }
    }
}

/// Get the `multipart/*` boundary string from `hyper::Headers`
pub fn get_multipart_boundary(headers: &HeaderMap) -> Result<Vec<u8>, Error> {
    // Verify that the request is 'Content-Type: multipart/*'.
    let mime = match headers.get("content-type") {
        Some(ct) => match ct.to_str() {
            Ok(value) => match Mime::from_str(value) {
                Ok(value) => value,
                Err(_) => return Err(Error::HeaderValueNotMime),
            },
            Err(err) => return Err(Error::ToStr(err)),
        },
        None => return Err(Error::NoRequestContentType),
    };
    let top_level = mime.type_();

    if top_level != mime::MULTIPART {
        return Err(Error::NotMultipart);
    }

    match mime.get_param(mime::BOUNDARY) {
        None => Err(Error::BoundaryNotSpecified),
        Some(content) => {
            let mut boundary = vec![];
            boundary.extend(b"--".iter().cloned());
            boundary.extend(content.to_string().as_bytes());
            Ok(boundary)
        }
    }
}

#[inline]
fn get_content_disposition_filename(cd: &HeaderValue) -> Result<Option<String>, Error> {
    match cd.to_str() {
        Ok(value) => match value.contains("filename") {
            true => match value.find("filename=") {
                Some(index) => {
                    let start = index + "filename=".len();
                    Ok(Some(
                        value.get(start..).unwrap().trim_matches('\"').to_owned(),
                    ))
                }
                None => match value.find("filename*=UTF-8''") {
                    Some(index) => {
                        let start = index + "filename*=UTF-8''".len();
                        Ok(Some(
                            value.get(start..).unwrap().trim_matches('\"').to_owned(),
                        ))
                    }
                    None => Ok(None),
                },
            },
            false => Ok(None),
        },
        Err(err) => Err(Error::ToStr(err)),
    }
}

/// Generate a valid multipart boundary, statistically unlikely to be found within
/// the content of the parts.
pub fn generate_boundary() -> Vec<u8> {
    TextNonce::sized(68)
        .unwrap()
        .into_string()
        .into_bytes()
        .iter()
        .map(|&ch| {
            if ch == b'=' {
                b'-'
            } else if ch == b'/' {
                b'.'
            } else {
                ch
            }
        })
        .collect()
}

// Convenience method, like write_all(), but returns the count of bytes written.
trait WriteAllCount {
    fn write_all_count(&mut self, buf: &[u8]) -> std::io::Result<usize>;
}
impl<T: Write> WriteAllCount for T {
    fn write_all_count(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.write_all(buf)?;
        Ok(buf.len())
    }
}

/// Stream a multipart body to the output `stream` given, made up of the `parts`
/// given.  Top-level headers are NOT included in this stream; the caller must send
/// those prior to calling write_multipart().
/// Returns the number of bytes written, or an error.
pub fn write_multipart<S: Write>(
    stream: &mut S,
    boundary: &[u8],
    nodes: &Vec<Node>,
) -> Result<usize, Error> {
    let mut count: usize = 0;

    for node in nodes {
        // write a boundary
        count += stream.write_all_count(b"--")?;
        count += stream.write_all_count(boundary)?;
        count += stream.write_all_count(b"\r\n")?;

        match *node {
            Node::Part(ref part) => {
                // write the part's headers
                for header in part.headers.iter() {
                    count += stream.write_all_count(header.0.as_str().as_bytes())?;
                    count += stream.write_all_count(b": ")?;
                    count += stream.write_all_count(header.1.as_bytes())?;
                    count += stream.write_all_count(b"\r\n")?;
                }

                // write the blank line
                count += stream.write_all_count(b"\r\n")?;

                // Write the part's content
                count += stream.write_all_count(&part.body)?;
            }
            Node::File(ref filepart) => {
                // write the part's headers
                for header in filepart.headers.iter() {
                    count += stream.write_all_count(header.0.as_str().as_bytes())?;
                    count += stream.write_all_count(b": ")?;
                    count += stream.write_all_count(header.1.as_bytes())?;
                    count += stream.write_all_count(b"\r\n")?;
                }

                // write the blank line
                count += stream.write_all_count(b"\r\n")?;

                // Write out the files's content
                let mut file = File::open(&filepart.path)?;
                count += std::io::copy(&mut file, stream)? as usize;
            }
            Node::Multipart((ref headers, ref subnodes)) => {
                // Get boundary
                let boundary = get_multipart_boundary(headers)?;

                // write the multipart headers
                for header in headers.iter() {
                    count += stream.write_all_count(header.0.as_str().as_bytes())?;
                    count += stream.write_all_count(b": ")?;
                    count += stream.write_all_count(header.1.as_bytes())?;
                    count += stream.write_all_count(b"\r\n")?;
                }

                // write the blank line
                count += stream.write_all_count(b"\r\n")?;

                // Recurse
                count += write_multipart(stream, &boundary, subnodes)?;
            }
        }

        // write a line terminator
        count += stream.write_all_count(b"\r\n")?;
    }

    // write a final boundary
    count += stream.write_all_count(b"--")?;
    count += stream.write_all_count(boundary)?;
    count += stream.write_all_count(b"--")?;

    Ok(count)
}

pub fn write_chunk<S: Write>(stream: &mut S, chunk: &[u8]) -> Result<(), ::std::io::Error> {
    write!(stream, "{:x}\r\n", chunk.len())?;
    stream.write_all(chunk)?;
    stream.write_all(b"\r\n")?;
    Ok(())
}

/// Stream a multipart body to the output `stream` given, made up of the `parts`
/// given, using Tranfer-Encoding: Chunked.  Top-level headers are NOT included in this
/// stream; the caller must send those prior to calling write_multipart_chunked().
pub fn write_multipart_chunked<S: Write>(
    stream: &mut S,
    boundary: &[u8],
    nodes: &Vec<Node>,
) -> Result<(), Error> {
    for node in nodes {
        // write a boundary
        write_chunk(stream, b"--")?;
        write_chunk(stream, boundary)?;
        write_chunk(stream, b"\r\n")?;

        match *node {
            Node::Part(ref part) => {
                // write the part's headers
                for header in part.headers.iter() {
                    write_chunk(stream, header.0.as_str().as_bytes())?;
                    write_chunk(stream, b": ")?;
                    write_chunk(stream, header.1.as_bytes())?;
                    write_chunk(stream, b"\r\n")?;
                }

                // write the blank line
                write_chunk(stream, b"\r\n")?;

                // Write the part's content
                write_chunk(stream, &part.body)?;
            }
            Node::File(ref filepart) => {
                // write the part's headers
                for header in filepart.headers.iter() {
                    write_chunk(stream, header.0.as_str().as_bytes())?;
                    write_chunk(stream, b": ")?;
                    write_chunk(stream, header.1.as_bytes())?;
                    write_chunk(stream, b"\r\n")?;
                }

                // write the blank line
                write_chunk(stream, b"\r\n")?;

                // Write out the files's length
                let metadata = std::fs::metadata(&filepart.path)?;
                write!(stream, "{:x}\r\n", metadata.len())?;

                // Write out the file's content
                let mut file = File::open(&filepart.path)?;
                std::io::copy(&mut file, stream)?;
                stream.write_all(b"\r\n")?;
            }
            Node::Multipart((ref headers, ref subnodes)) => {
                // Get boundary
                let boundary = get_multipart_boundary(headers)?;

                // write the multipart headers
                for header in headers.iter() {
                    write_chunk(stream, header.0.as_str().as_bytes())?;
                    write_chunk(stream, b": ")?;
                    write_chunk(stream, header.1.as_bytes())?;
                    write_chunk(stream, b"\r\n")?;
                }

                // write the blank line
                write_chunk(stream, b"\r\n")?;

                // Recurse
                write_multipart_chunked(stream, &boundary, subnodes)?;
            }
        }

        // write a line terminator
        write_chunk(stream, b"\r\n")?;
    }

    // write a final boundary
    write_chunk(stream, b"--")?;
    write_chunk(stream, boundary)?;
    write_chunk(stream, b"--")?;

    // Write an empty chunk to signal the end of the body
    write_chunk(stream, b"")?;

    Ok(())
}
