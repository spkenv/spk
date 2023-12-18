// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use miette::Diagnostic;

/// A specialized result for encoding operations
pub type Result<T> = std::result::Result<T, Error>;

/// The error type that is returned by encoding operations
#[derive(thiserror::Error, Diagnostic, Debug)]
#[diagnostic(
    url(
        "https://getspk.io/error_codes#{}",
        self.code().unwrap_or_else(|| Box::new("spfs::generic"))
    )
)]
pub enum Error {
    /// Some underlying io error caused a decode process to fail
    #[error("Encoding read error")]
    FailedRead(#[source] std::io::Error),

    /// Some underlying io error caused a encode process to fail
    #[error("Encoding write error")]
    FailedWrite(#[source] std::io::Error),

    /// A string could not be decoded because of an invalid byte sequence
    #[error("Error in encoding format")]
    InvalidStringEncoding(#[source] std::str::Utf8Error),

    /// Strings cannot be encoded by this crate if they contain
    /// a null character, as that character is used as terminating character
    #[error("Cannot encode string with null character")]
    StringHasNull,

    /// The header in a byte stream was not as expected
    #[error("Invalid header: wanted '{wanted:?}', got '{got:?}'")]
    InvalidHeader {
        /// The header sequence that was desired
        wanted: Vec<u8>,
        /// The actual observed sequence of bytes
        got: Vec<u8>,
    },

    /// An error occurred with the digest protocol format
    #[error(transparent)]
    #[diagnostic(forward(0))]
    Digest(#[from] spfs_proto::digest::Error),

    /// A partial digest could not be parsed from a string because
    /// of some issue with the provided data
    #[error("Invalid partial digest '{given}': {reason}")]
    InvalidPartialDigest {
        /// The reason that the digest string was invalid
        reason: String,
        /// A copy of the invalid string
        given: String,
    },
}
