// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

//! # Spfs Encoding Library
//!
//! This crate defines how spfs hashes data and generates
//! each content [`Digest`]. Additionally, the [`Encodable`] and
//! [`Decodable`] traits are used for objects that can be hashed
//! along with a number of functions related to the custom flavor
//! of binary encoding that spfs uses for its internal data types.

#![deny(missing_docs)]

mod binary;
mod error;
mod hash;

pub use binary::{
    consume_header,
    read_digest,
    read_int,
    read_string,
    read_uint8,
    read_uint64,
    write_digest,
    write_header,
    write_int,
    write_string,
    write_uint8,
    write_uint64,
};
pub use error::{Error, Result};
pub use hash::{Decodable, Digestible, Encodable, Hasher, PartialDigest};
pub use spfs_proto::{DIGEST_SIZE, Digest, EMPTY_DIGEST, NULL_DIGEST, parse_digest};

/// # Encoding Prelude
///
/// A collection of traits commonly used from this crate.
pub mod prelude {
    pub use super::{Decodable, Digestible, Encodable};
}
