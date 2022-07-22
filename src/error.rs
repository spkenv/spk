// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use thiserror::Error;

use crate::solve;

use super::{api, build, test};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    IO(#[from] std::io::Error),
    #[error(transparent)]
    SPFS(#[from] spfs::Error),
    #[error(transparent)]
    Solve(#[from] crate::solve::Error),
    #[error("Error: {0}")]
    String(String),

    // API Errors
    #[error(transparent)]
    InvalidVersionError(#[from] api::InvalidVersionError),
    #[error(transparent)]
    InvalidNameError(#[from] api::InvalidNameError),
    #[error(transparent)]
    InvalidBuildError(#[from] api::InvalidBuildError),
    #[error("Invalid package spec for {0}: {1}")]
    InvalidPackageSpec(api::Ident, #[source] serde_yaml::Error),
    #[error("Failed to encode spec: {0}")]
    SpecEncodingError(#[source] serde_yaml::Error),
    #[error("Invalid package spec file {0}: {1}")]
    InvalidPackageSpecFile(std::path::PathBuf, #[source] serde_yaml::Error),
    #[error("Invalid repository metadata: {0}")]
    InvalidRepositoryMetadata(#[source] serde_yaml::Error),
    #[error("Invalid inheritance: {0}")]
    InvalidInheritance(#[source] serde_yaml::Error),
    #[error("Invalid PreReleasePolicy: {0}")]
    InvalidPreReleasePolicy(#[source] serde_yaml::Error),
    #[error("Invalid InclusionPolicy: {0}")]
    InvalidInclusionPolicy(#[source] serde_yaml::Error),

    // Storage Errors
    #[error("Package not found: {0}")]
    PackageNotFoundError(api::Ident),
    #[error("Version exists: {0}")]
    VersionExistsError(api::Ident),

    // Build Errors
    #[error(transparent)]
    Collection(#[from] build::CollectionError),
    #[error(transparent)]
    Build(#[from] build::BuildError),

    // Bake Errors
    #[error("Skip embedded")]
    SkipEmbedded,

    // Test Errors
    #[error(transparent)]
    Test(#[from] test::TestError),

    /// Not running under an active spk environment
    #[error("No current spfs runtime environment")]
    NoEnvironment,
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

impl From<solve::graph::GraphError> for Error {
    fn from(err: solve::graph::GraphError) -> Error {
        Error::Solve(err.into())
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
