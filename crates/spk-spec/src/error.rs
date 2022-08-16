// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Invalid inheritance: {0}")]
    InvalidInheritance(#[source] serde_yaml::Error),
    #[error("Invalid package spec file {0}: {1}")]
    InvalidPackageSpecFile(std::path::PathBuf, #[source] serde_yaml::Error),
    #[error(transparent)]
    IO(#[from] std::io::Error),
    #[error("Failed to encode spec: {0}")]
    SpecEncodingError(#[source] serde_yaml::Error),
    #[error(transparent)]
    SPFS(#[from] spfs::Error),
    #[error(transparent)]
    SpkBuildOpsError(#[from] spk_build_ops::Error),
    #[error(transparent)]
    SpkIdentBuildError(#[from] spk_foundation::ident_build::Error),
    #[error(transparent)]
    SpkIdentComponentError(#[from] spk_foundation::ident_component::Error),
    #[error(transparent)]
    SpkIdentError(#[from] spk_ident::Error),
    #[error(transparent)]
    SpkNameError(#[from] spk_foundation::name::Error),
    #[error("Error: {0}")]
    String(String),
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
