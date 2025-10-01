// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use miette::Diagnostic;
use spk_schema::{AnyIdent, VersionIdent};
use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub struct InvalidPackageSpec(
    pub AnyIdent,
    // ideally this would contain the original format_serde_error instance
    // but they are not clone-able and we need to be able to cache and duplicate
    // this error type
    pub String,
);

impl std::fmt::Display for InvalidPackageSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Invalid package spec for {}: {}", self.0, self.1)
    }
}

#[derive(Diagnostic, Debug, Error)]
#[diagnostic(
    url(
        "https://spkenv.dev/error_codes#{}",
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
    #[error("{0}")]
    InvalidPackageSpec(Box<InvalidPackageSpec>),
    #[error("Invalid repository metadata: {0}")]
    InvalidRepositoryMetadata(#[source] serde_yaml::Error),
    #[error("Package not found: {0}")]
    PackageNotFound(Box<AnyIdent>),
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
    SpkSpecError(Box<spk_schema::Error>),
    #[error(transparent)]
    #[diagnostic(forward(0))]
    SpkWorkspaceFromPathError(#[from] spk_workspace::error::FromPathError),
    #[error(transparent)]
    #[diagnostic(forward(0))]
    SpkWorkspaceBuildError(#[from] spk_workspace::error::BuildError),
    #[error("No disk usage: version '{0}' not found")]
    DiskUsageVersionNotFound(String),
    #[error("No disk usage: build '{0}' not found")]
    DiskUsageBuildNotFound(String),
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

impl From<spk_schema::Error> for Error {
    fn from(err: spk_schema::Error) -> Error {
        Error::SpkSpecError(Box::new(err))
    }
}
