// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

mod binary;
pub use binary::{
    consume_header, read_digest, read_int, read_string, read_uint, write_digest, write_header,
    write_int, write_string, write_uint, INT_SIZE,
};

mod error;
pub use error::{Error, Result};

mod hash;
pub use hash::{
    parse_digest, Decodable, Digest, Encodable, Hasher, PartialDigest, DIGEST_SIZE, EMPTY_DIGEST,
    NULL_DIGEST,
};

pub mod prelude;
