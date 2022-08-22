// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    InvalidVersionError(#[from] super::InvalidVersionError),
    #[error(transparent)]
    SpkNameError(#[from] crate::name::Error),
    #[error("Error: {0}")]
    String(String),
}
