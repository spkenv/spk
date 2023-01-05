// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use spk_schema::AnyIdent;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Failed to create directory {0}")]
    DirectoryCreateError(std::path::PathBuf, #[source] std::io::Error),
    #[error("Failed to open file {0}")]
    FileOpenError(std::path::PathBuf, #[source] std::io::Error),
    #[error("Failed to read file {0}")]
    FileReadError(std::path::PathBuf, #[source] std::io::Error),
    #[error("Invalid package spec for {0}: {1}")]
    InvalidPackageSpec(
        AnyIdent,
        // ideally this would contain the original format_serde_error instance
        // but they are not clone-able and we need to be able to cache and duplicate
        // this error type
        String,
    ),
    #[error("Invalid repository metadata: {0}")]
    InvalidRepositoryMetadata(#[source] serde_yaml::Error),
    #[error(transparent)]
    SPFS(#[from] spfs::Error),
    #[error(transparent)]
    SpkIdentError(#[from] spk_schema::ident::Error),
    #[error(transparent)]
    SpkIdentBuildError(#[from] spk_schema::foundation::ident_build::Error),
    #[error(transparent)]
    SpkIdentComponentError(#[from] spk_schema::foundation::ident_component::Error),
    #[error(transparent)]
    SpkNameError(#[from] spk_schema::foundation::name::Error),
    #[error(transparent)]
    SpkSpecError(#[from] spk_schema::Error),
    #[error(transparent)]
    SpkValidatorsError(#[from] spk_schema::validators::Error),
    #[error("Error: {0}")]
    String(String),
}

impl From<String> for Error {
    fn from(err: String) -> Error {
        Error::String(err)
    }
}

impl From<&str> for Error {
    fn from(err: &str) -> Error {
        Error::String(err.to_owned())
    }
}
