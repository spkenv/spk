// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

pub type Result<T> = std::result::Result<T, Error>;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Encoding read error")]
    EncodingReadError(#[source] std::io::Error),
    #[error("Encoding write error")]
    EncodingWriteError(#[source] std::io::Error),
    #[error("Error in encoding format")]
    EncodingFormatError(#[source] std::str::Utf8Error),
    #[error("Cannot encode string with null character")]
    StringHasNullCharacter,

    #[error("Invalid header: wanted '{wanted:?}', got '{got:?}'")]
    InvalidHeader { wanted: Vec<u8>, got: Vec<u8> },

    #[error("Could not decode digest: {0}")]
    DigestDecodeError(#[source] data_encoding::DecodeError),
    #[error("Invalid number of bytes for digest: {0} != {}", super::DIGEST_SIZE)]
    DigestLengthError(usize),

    #[error("Invalid partial digest '{given}': {reason}")]
    InvalidPartialDigest { reason: String, given: String },
}
