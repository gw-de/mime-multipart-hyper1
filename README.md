# mime-multipart

[![MIT licensed](https://img.shields.io/badge/license-MIT-blue.svg)](./LICENSE-MIT)
[![Apache-2.0 licensed](https://img.shields.io/badge/license-APACHE2-blue.svg)](./LICENSE-APACHE)

Rust library for MIME multipart parsing, construction, and streaming

This is a fork of https://github.com/mikedilger/mime-multipart with support for newer hyper versions.

Documentation is available at https://docs.rs/mime-multipart

## Compatibility

* **Version 0.8**
  * Use Rust version 2021
  * Update hyper from 0.10 to 0.11
  * Updates mime crate to version 0.3 (up-to-date at time of writing, November 2024)
* **Version 0.9**
  * Update hyper to 0.14 (also compatible with hyper 0.12 and 0.13)

## Features

* Parses from a stream, rather than in memory, so that memory is not hogged.
* Streams parts which are identified as files (via the part's Content-Disposition header,
  if any, or via a manual override) to files on disk.
* Uses buffered streams.
* Lets you build and stream out a multipart as a vector of parts (`Node`s), some of which
  could be files, others could be nested multipart parts.

If you are specifically dealing with `multipart/formdata`, you may be interested in
https://github.com/mikedilger/formdata which uses this crate and takes it a step
further.

## License

Licensed under either of

* Apache License, Version 2.0
  ([LICENSE-APACHE](LICENSE-APACHE)
  or http://www.apache.org/licenses/LICENSE-2.0)
* MIT license
  ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall
be dual licensed as above, without any additional terms or conditions.
