// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Failed to open file {0}")]
    FileOpenError(std::path::PathBuf, #[source] std::io::Error),
    #[error("Failed to write file {0}")]
    FileWriteError(std::path::PathBuf, #[source] std::io::Error),
    #[error("Invalid inheritance: {0}")]
    InvalidInheritance(#[source] serde_yaml::Error),
    #[error("Invalid package spec file {0}: {1}")]
    InvalidPackageSpecFile(std::path::PathBuf, #[source] serde_yaml::Error),
    #[error("Invalid path {0}")]
    InvalidPath(std::path::PathBuf, #[source] std::io::Error),
    #[error("Failed to spawn {0} process")]
    ProcessSpawnError(String, #[source] std::io::Error),
    #[error("Failed to wait for process: {0}")]
    ProcessWaitError(#[source] std::io::Error),
    #[error("Failed to encode spec: {0}")]
    SpecEncodingError(#[source] serde_yaml::Error),
    #[error(transparent)]
    SPFS(#[from] spfs::Error),
    #[error(transparent)]
    SpkIdentBuildError(#[from] crate::foundation::ident_build::Error),
    #[error(transparent)]
    SpkIdentComponentError(#[from] crate::foundation::ident_component::Error),
    #[error(transparent)]
    SpkIdentError(#[from] crate::ident::Error),
    #[error(transparent)]
    SpkNameError(#[from] crate::foundation::name::Error),
    #[error("Error: {0}")]
    String(String),
    #[error("Failed to create temp dir: {0}")]
    TempDirError(#[source] std::io::Error),
}

impl Error {
    /// Wraps an error message with a prefix, creating a contextual but generic error
    pub fn wrap<S: AsRef<str>>(prefix: S, err: Self) -> Self {
        Error::String(format!("{}: {:?}", prefix.as_ref(), err))
    }

    /// Wraps an error message with a prefix, creating a contextual error
    pub fn wrap_io<S: AsRef<str>>(prefix: S, err: std::io::Error) -> Error {
        Error::String(format!("{}: {:?}", prefix.as_ref(), err))
    }
}
