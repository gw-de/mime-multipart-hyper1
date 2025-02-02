// Copyright 2016 mime-multipart Developers
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

use std::error::Error as StdError;
use std::fmt::{self, Display};
use std::io;
use std::string::FromUtf8Error;

use http;
use http::header::ToStrError;
use httparse;

/// An error type for the `mime-multipart` crate.
pub enum Error {
    /// The Hyper request did not have a Content-Type header.
    NoRequestContentType,
    /// The Hyper request Content-Type top-level Mime was not `Multipart`.
    NotMultipart,
    /// The Content-Type header failed to specify boundary token.
    BoundaryNotSpecified,
    /// A multipart section contained only partial headers.
    PartialHeaders,
    EofInMainHeaders,
    EofBeforeFirstBoundary,
    NoCrLfAfterBoundary,
    EofInPartHeaders,
    EofInFile,
    EofInPart,
    HeaderMissing,
    InvalidHeaderNameOrValue,
    HeaderValueNotMime,
    FilenameWithNonAsciiEncodingNotSupported,
    ToStr(ToStrError),
    /// An HTTP parsing error from a multipart section.
    Httparse(httparse::Error),
    /// An I/O error.
    Io(io::Error),
    /// An error was returned from Hyper.
    Http(http::Error),
    /// An error occurred during UTF-8 processing.
    Utf8(FromUtf8Error),
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Error {
        Error::Io(err)
    }
}

impl From<httparse::Error> for Error {
    fn from(err: httparse::Error) -> Error {
        Error::Httparse(err)
    }
}

impl From<http::Error> for Error {
    fn from(err: http::Error) -> Error {
        Error::Http(err)
    }
}

impl From<FromUtf8Error> for Error {
    fn from(err: FromUtf8Error) -> Error {
        Error::Utf8(err)
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Error::Httparse(ref e) => format!("Httparse: {:?}", e).fmt(f),
            Error::Io(ref e) => format!("Io: {}", e).fmt(f),
            Error::Http(ref e) => format!("Http: {}", e).fmt(f),
            Error::Utf8(ref e) => format!("Utf8: {}", e).fmt(f),
            Error::ToStr(ref e) => format!("ToStr: {}", e).fmt(f),
            Error::NoRequestContentType => "NoRequestContentType".to_string().fmt(f),
            Error::NotMultipart => "NotMultipart".to_string().fmt(f),
            Error::BoundaryNotSpecified => "BoundaryNotSpecified".to_string().fmt(f),
            Error::PartialHeaders => "PartialHeaders".to_string().fmt(f),
            Error::EofBeforeFirstBoundary => "EofBeforeFirstBoundary".to_string().fmt(f),
            Error::NoCrLfAfterBoundary => "NoCrLfAfterBoundary".to_string().fmt(f),
            Error::EofInPartHeaders => "EofInPartHeaders".to_string().fmt(f),
            Error::EofInFile => "EofInFile".to_string().fmt(f),
            Error::EofInPart => "EofInPart".to_string().fmt(f),
            Error::EofInMainHeaders => "EofInMainHeaders".to_string().fmt(f),
            Error::HeaderMissing => "HeaderMissing".to_string().fmt(f),
            Error::InvalidHeaderNameOrValue => "InvalidHeaderNameOrValue".to_string().fmt(f),
            Error::HeaderValueNotMime => "HeaderValueNotMime".to_string().fmt(f),
            Error::FilenameWithNonAsciiEncodingNotSupported => {
                "NonAsciiFilenameNotSupported".to_string().fmt(f)
            }
        }
    }
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self)?;
        if self.source().is_some() {
            write!(f, ": {:?}", self.source().unwrap())?; // recurse
        }
        Ok(())
    }
}

impl StdError for Error {
    fn description(&self) -> &str {
        match *self {
            Error::NoRequestContentType => "The Hyper request did not have a Content-Type header.",
            Error::NotMultipart => {
                "The Hyper request Content-Type top-level Mime was not multipart."
            }
            Error::BoundaryNotSpecified => {
                "The Content-Type header failed to specify a boundary token."
            }
            Error::PartialHeaders => "A multipart section contained only partial headers.",
            Error::EofInMainHeaders => "The request headers ended pre-maturely.",
            Error::EofBeforeFirstBoundary => {
                "The request body ended prior to reaching the expected starting boundary."
            }
            Error::NoCrLfAfterBoundary => "Missing CRLF after boundary.",
            Error::EofInPartHeaders => {
                "The request body ended prematurely while parsing headers of a multipart part."
            }
            Error::EofInFile => "The request body ended prematurely while streaming a file part.",
            Error::EofInPart => {
                "The request body ended prematurely while reading a multipart part."
            }
            Error::Httparse(_) => {
                "A parse error occurred while parsing the headers of a multipart section."
            }
            Error::Io(_) => "An I/O error occurred.",
            Error::Http(_) => "A Http error occurred.",
            Error::Utf8(_) => "A UTF-8 error occurred.",
            Error::HeaderMissing => "The requested header could not be found in the HeaderMap",
            Error::InvalidHeaderNameOrValue => "Parsing to HeaderName or HeaderValue failed",
            Error::HeaderValueNotMime => "HeaderValue could not be parsed to Mime",
            Error::ToStr(_) => "A ToStr error occurred.",
            Error::FilenameWithNonAsciiEncodingNotSupported => {
                "Non-ASCII filename parsing not supported"
            }
        }
    }
}
