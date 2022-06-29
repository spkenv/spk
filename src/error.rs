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
    Serde(#[from] serde_yaml::Error),
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
