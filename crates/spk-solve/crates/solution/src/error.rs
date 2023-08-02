// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    SpkStorageError(#[from] spk_storage::Error),
    #[error("Embedded has no component layers")]
    EmbeddedHasNoComponentLayers,
    #[error("Spk internal test has no component layers")]
    SpkInternalTestHasNoComponentLayers,
    #[error("Error: {0}")]
    String(String),
}
