// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use miette::Diagnostic;
use spk_schema::{AnyIdent, VersionIdent};
use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Diagnostic, Debug, Error)]
#[diagnostic(
    url(
        "https://getspk.io/error_codes#{}",
        self.code().unwrap_or_else(|| Box::new("spk::generic"))
    )
)]
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
    #[error("Package not found: {0}")]
    PackageNotFound(AnyIdent),
    #[error("Version exists: {0}")]
    VersionExists(VersionIdent),
    #[error(transparent)]
    #[diagnostic(forward(0))]
    SPFS(#[from] spfs::Error),
    #[error(transparent)]
    #[diagnostic(forward(0))]
    SpkIdentError(#[from] spk_schema::ident::Error),
    #[error(transparent)]
    #[diagnostic(forward(0))]
    SpkIdentBuildError(#[from] spk_schema::foundation::ident_build::Error),
    #[error(transparent)]
    #[diagnostic(forward(0))]
    SpkIdentComponentError(#[from] spk_schema::foundation::ident_component::Error),
    #[error(transparent)]
    #[diagnostic(forward(0))]
    SpkNameError(#[from] spk_schema::foundation::name::Error),
    #[error(transparent)]
    #[diagnostic(forward(0))]
    SpkSpecError(#[from] spk_schema::Error),
    #[error("{0}")]
    String(String),
}

impl Error {
    /// Return true if this is a `PackageNotFound` error.
    #[inline]
    pub fn is_package_not_found(&self) -> bool {
        matches!(self, Self::PackageNotFound(_))
    }
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
