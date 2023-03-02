// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

//! # Spfs Encoding Library
//!
//! This crate defines how spfs hashes data and generates
//! each content [`Digest`]. Additionally, the [`Encodable`] and
//! [`Decodable`] traits are used for objects that can be hashed
//! along with a number of functions related to the custom flavor
//! of binary encoding that spfs uses for its internal data types.

#![deny(missing_docs)]
#![deny(unsafe_op_in_unsafe_fn)]
#![warn(clippy::fn_params_excessive_bools)]

mod binary;
mod error;
mod hash;

pub use binary::{
    consume_header,
    read_digest,
    read_int,
    read_string,
    read_uint,
    write_digest,
    write_header,
    write_int,
    write_string,
    write_uint,
};
pub use error::{Error, Result};
pub use hash::{
    parse_digest,
    Decodable,
    Digest,
    Encodable,
    Hasher,
    PartialDigest,
    DIGEST_SIZE,
    EMPTY_DIGEST,
    NULL_DIGEST,
};

/// # Encoding Prelude
///
/// A collection of traits commonly used from this crate.
pub mod prelude {
    pub use super::{Decodable, Encodable};
}
