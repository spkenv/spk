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
    SpkConfigError(#[from] spk_config::Error),
    #[error("No disk usage: version '{0}' not found")]
    DiskUsageVersionNotFound(String),
    #[error("No disk usage: build '{0}' not found")]
    DiskUsageBuildNotFound(String),

    #[error("Unable to open flatbuffer index file for repo: {0}")]
    IndexOpenError(#[source] std::io::Error),
    #[error("Unable to memory map flatbuffer index from repo file: {0}")]
    IndexMemMapError(#[source] std::io::Error),
    #[error("Unable to write '{0}' repo's index, '{1}': {2}")]
    IndexWriteError(String, String, #[source] std::io::Error),
    #[error(
        "Cannot generate an index from this repo: It is not a spk MemoryRepository or SpfsRepository"
    )]
    IndexGenerationInMemError(),
    #[error("'{0}' repo does not have a index file location: {1}")]
    IndexNoRepoPathError(String, String),
    #[error("No index location for the '{0}' repo. It is a {1} repository")]
    IndexNoRepoLocationError(String, String),
    #[error("Failed to load flatbuffer index: {0}")]
    IndexFailedToLoad(String),
    #[error("Failed to generate flatbuffer index in memory: {0}")]
    IndexFailedToGenerate(String),
    #[error("Unknown index kind: '{0}', unable to {1}load that kind of index")]
    IndexUnknownKind(String, String),
    #[error("Unable to open the {0} lock file at all: {1}: {2}")]
    UnableToOpenLockFileError(String, String, String),
    #[error(
        "Unable to lock the {0} file exclusively. {1} tries with {2} seconds between each. Lock is held by {3} for {4}. Giving up. Try again later, or investigate the process that made the lock file."
    )]
    UnableToGetWriteLockError(String, u64, u64, String, String),
    #[error("Failed to remove the {0} lock file: {1}: {2}")]
    UnableToRemoveWriteLockError(String, String, String),

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
