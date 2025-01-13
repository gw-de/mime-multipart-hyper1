// Copyright 2016 mime-multipart Developers
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

use super::*;

use http::header::{HeaderMap, HeaderValue, CONTENT_DISPOSITION, CONTENT_TYPE};

#[test]
fn parser() {
    let input = b"POST / HTTP/1.1\r\n\
                  Host: example.domain\r\n\
                  Content-Type: multipart/mixed; boundary=\"abcdefg\"\r\n\
                  Content-Length: 1000\r\n\
                  \r\n\
                  --abcdefg\r\n\
                  Content-Type: application/json\r\n\
                  \r\n\
                  {\r\n\
                    \"id\": 15\r\n\
                  }\r\n\
                  --abcdefg\r\n\
                  Content-Disposition: Attachment; filename=\"image.gif\"\r\n\
                  Content-Type: image/gif\r\n\
                  \r\n\
                  This is a file\r\n\
                  with two lines\r\n\
                  --abcdefg\r\n\
                  Content-Disposition: Attachment; filename=\"file.txt\"\r\n\
                  \r\n\
                  This is a file\r\n\
                  --abcdefg--";

    let mut raw_headers = [httparse::EMPTY_HEADER; 16];
    let mut req = httparse::Request::new(&mut raw_headers);
    let res = req.parse(input).unwrap();
    let body_start = res.unwrap();

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
            Err(_) => panic!("Issue converting headers"),
        };

        let header_name = header.name.to_owned();
        println!("{}", header_name);
        let header_name = match HeaderName::from_str(&header_name) {
            Ok(value) => value,
            Err(_) => panic!("Issue converting headers"),
        };
        headers.append(header_name, header_value);
    }

    let body = input[body_start..].to_vec();

    match read_multipart_body(&mut &*body, &headers, false) {
        Ok(nodes) => {
            assert_eq!(nodes.len(), 3);

            if let Node::Part(ref part) = nodes[0] {
                assert_eq!(
                    part.body,
                    b"{\r\n\
                                          \"id\": 15\r\n\
                                        }"
                );
            } else {
                panic!("1st node of wrong type");
            }

            if let Node::File(ref filepart) = nodes[1] {
                assert_eq!(filepart.size, Some(30));
                assert_eq!(filepart.filename().unwrap().unwrap(), "image.gif");
                assert_eq!(filepart.content_type().unwrap(), mime::IMAGE_GIF);

                assert!(filepart.path.exists());
                assert!(filepart.path.is_file());
            } else {
                panic!("2nd node of wrong type");
            }

            if let Node::File(ref filepart) = nodes[2] {
                assert_eq!(filepart.size, Some(14));
                assert_eq!(filepart.filename().unwrap().unwrap(), "file.txt");
                assert!(filepart.content_type().is_none());

                assert!(filepart.path.exists());
                assert!(filepart.path.is_file());
            } else {
                panic!("3rd node of wrong type");
            }
        }
        Err(err) => panic!("{}", err),
    }
}

#[test]
fn mixed_parser() {
    let input = b"POST / HTTP/1.1\r\n\
                  Host: example.domain\r\n\
                  Content-Type: multipart/form-data; boundary=AaB03x\r\n\
                  Content-Length: 1000\r\n\
                  \r\n\
                  --AaB03x\r\n\
                  Content-Disposition: form-data; name=\"submit-name\"\r\n\
                  \r\n\
                  Larry\r\n\
                  --AaB03x\r\n\
                  Content-Disposition: form-data; name=\"files\"\r\n\
                  Content-Type: multipart/mixed; boundary=BbC04y\r\n\
                  \r\n\
                  --BbC04y\r\n\
                  Content-Disposition: file; filename=\"file1.txt\"\r\n\
                  \r\n\
                  ... contents of file1.txt ...\r\n\
                  --BbC04y\r\n\
                  Content-Disposition: file; filename=\"awesome_image.gif\"\r\n\
                  Content-Type: image/gif\r\n\
                  Content-Transfer-Encoding: binary\r\n\
                  \r\n\
                  ... contents of awesome_image.gif ...\r\n\
                  --BbC04y--\r\n\
                  --AaB03x--";

    let mut raw_headers = [httparse::EMPTY_HEADER; 16];
    let mut req = httparse::Request::new(&mut raw_headers);
    let res = req.parse(input).unwrap();
    let body_start = res.unwrap();

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
            Err(_) => panic!("Issue converting headers"),
        };

        let header_name = header.name.to_owned();
        println!("{}", header_name);
        let header_name = match HeaderName::from_str(&header_name) {
            Ok(value) => value,
            Err(_) => panic!("Issue converting headers"),
        };
        headers.append(header_name, header_value);
    }

    let body = input[body_start..].to_vec();

    match read_multipart_body(&mut &*body, &headers, false) {
        Ok(nodes) => {
            assert_eq!(nodes.len(), 2);

            if let Node::Part(ref part) = nodes[0] {
                let cd_name = match part.headers.get("content-disposition") {
                    Some(cd) => get_content_disposition_name(cd),
                    None => None,
                };
                assert_eq!(&*cd_name.unwrap(), "submit-name");
                assert_eq!(::std::str::from_utf8(&part.body).unwrap(), "Larry");
            } else {
                panic!("1st node of wrong type");
            }

            if let Node::Multipart((ref headers, ref subnodes)) = nodes[1] {
                let cd_name = match headers.get("content-disposition") {
                    Some(cd) => get_content_disposition_name(cd),
                    None => None,
                };
                assert_eq!(&*cd_name.unwrap(), "files");

                assert_eq!(subnodes.len(), 2);

                if let Node::File(ref filepart) = subnodes[0] {
                    assert_eq!(filepart.size, Some(29));
                    assert_eq!(filepart.filename().unwrap().unwrap(), "file1.txt");
                    assert!(filepart.content_type().is_none());

                    assert!(filepart.path.exists());
                    assert!(filepart.path.is_file());
                } else {
                    panic!("1st subnode of wrong type");
                }

                if let Node::File(ref filepart) = subnodes[1] {
                    assert_eq!(filepart.size, Some(37));
                    assert_eq!(filepart.filename().unwrap().unwrap(), "awesome_image.gif");
                    assert_eq!(filepart.content_type().unwrap(), mime::IMAGE_GIF);

                    assert!(filepart.path.exists());
                    assert!(filepart.path.is_file());
                } else {
                    panic!("2st subnode of wrong type");
                }
            } else {
                panic!("2st node of wrong type");
            }
        }
        Err(err) => panic!("{}", err),
    }
}

#[test]
fn test_line_feed() {
    let input = b"POST /test HTTP/1.1\r\n\
                  Host: example.domain\r\n\
                  Cookie: session_id=a36ZVwAAAACDQ9gzBCzDVZ1VNrnZEI1U\r\n\
                  Content-Type: multipart/form-data; boundary=\"ABCDEFG\"\r\n\
                  Content-Length: 10000\r\n\
                  \r\n\
                  --ABCDEFG\n\
                  Content-Disposition: form-data; name=\"consignment_id\"\n\
                  \n\
                  4\n\
                  --ABCDEFG\n\
                  Content-Disposition: form-data; name=\"note\"\n\
                  \n\
                  Check out this file about genomes!\n\
                  --ABCDEFG\n\
                  Content-Type: text/plain\n\
                  Content-Disposition: attachment; filename=genome.txt\n\
                  \n\
                  This is a text file about genomes, apparently.\n\
                  Read on.\n\
                  --ABCDEFG--";

    let mut raw_headers = [httparse::EMPTY_HEADER; 16];
    let mut req = httparse::Request::new(&mut raw_headers);
    let res = req.parse(input).unwrap();
    let body_start = res.unwrap();

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
            Err(err) => panic!("Issue converting headers. Err: {:?}", err.to_string()),
        };

        let header_name = header.name.to_owned();
        let header_name = match HeaderName::from_str(&header_name) {
            Ok(value) => value,
            Err(err) => panic!("Issue converting headers. Err: {:?}", err.to_string()),
        };
        headers.append(header_name, header_value);
    }

    let body = input[body_start..].to_vec();

    if let Err(e) = read_multipart_body(&mut &*body, &headers, false) {
        panic!("{}", e);
    }
}

#[inline]
fn get_content_disposition_name(cd: &HeaderValue) -> Option<String> {
    match cd.to_str() {
        Ok(value) => match value.contains("name") {
            true => match value.find("name=") {
                Some(index) => {
                    let start = index + "name=".len();
                    Some(value.get(start..).unwrap().trim_matches('\"').to_owned())
                }
                None => match value.find("name*=UTF-8''") {
                    Some(index) => {
                        let start = index + "name*=UTF-8''".len();
                        Some(value.get(start..).unwrap().trim_matches('\"').to_owned())
                    }
                    None => None,
                },
            },
            false => None,
        },
        Err(_) => None,
    }
}

#[test]
fn test_output() {
    let mut output: Vec<u8> = Vec::new();
    let boundary = generate_boundary();

    let first_name = Part {
        headers: {
            let mut h = HeaderMap::new();
            h.append(CONTENT_TYPE, HeaderValue::from_str("text/plain").unwrap());
            h.append(
                CONTENT_DISPOSITION,
                HeaderValue::from_bytes(b"form-data; name=\"first_name\"").unwrap(),
            );
            h
        },
        body: b"Michael".to_vec(),
    };

    let last_name = Part {
        headers: {
            let mut h = HeaderMap::new();
            h.append(CONTENT_TYPE, HeaderValue::from_str("text/plain").unwrap());
            h.append(
                CONTENT_DISPOSITION,
                HeaderValue::from_bytes(b"form-data; name=\"last_name\"").unwrap(),
            );
            h
        },
        body: b"Dilger".to_vec(),
    };

    let nodes: Vec<Node> = vec![Node::Part(first_name), Node::Part(last_name)];

    let count = match write_multipart(&mut output, &boundary, &nodes) {
        Ok(c) => c,
        Err(e) => panic!("{:?}", e),
    };
    assert_eq!(count, output.len());

    let string = String::from_utf8_lossy(&output);

    // Hard to compare programmatically since the headers could come in any order.
    println!("{}", string);

    assert_eq!(output.len(), 390);
}

#[test]
fn test_chunked() {
    let mut output: Vec<u8> = Vec::new();
    let boundary = generate_boundary();

    let first_name = Part {
        headers: {
            let mut h = HeaderMap::new();
            h.append(CONTENT_TYPE, HeaderValue::from_str("text/plain").unwrap());
            h.append(
                CONTENT_DISPOSITION,
                HeaderValue::from_bytes(b"form-data; name=\"first_name\"").unwrap(),
            );
            h
        },
        body: b"Michael".to_vec(),
    };

    let last_name = Part {
        headers: {
            let mut h = HeaderMap::new();
            h.append(CONTENT_TYPE, HeaderValue::from_str("text/plain").unwrap());
            h.append(
                CONTENT_DISPOSITION,
                HeaderValue::from_bytes(b"form-data; name=\"last_name\"").unwrap(),
            );
            h
        },
        body: b"Dilger".to_vec(),
    };

    let nodes: Vec<Node> = vec![Node::Part(first_name), Node::Part(last_name)];

    assert!(write_multipart_chunked(&mut output, &boundary, &nodes).is_ok());

    let string = String::from_utf8_lossy(&output);

    // Hard to compare programmatically since the headers could come in any order.
    println!("{}", string);

    assert_eq!(output.len(), 557);
}
