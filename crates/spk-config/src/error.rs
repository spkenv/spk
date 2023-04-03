// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::io;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Invalid path {0}")]
    InvalidPath(std::path::PathBuf, #[source] io::Error),

    #[error("Cannot load config, lock has been poisoned: {0}")]
    LockPoisonedRead(String),
    #[error("Cannot update config, lock has been poisoned: {0}")]
    LockPoisonedWrite(String),

    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Config(#[from] config::ConfigError),
}

pub type Result<T> = std::result::Result<T, Error>;
