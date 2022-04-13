// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use crate::solve;

use super::{api, build, test};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    IO(std::io::Error),
    SPFS(spfs::Error),
    Serde(serde_yaml::Error),
    Solve(crate::solve::Error),
    String(String),

    // API Errors
    InvalidVersionError(api::InvalidVersionError),
    InvalidNameError(api::InvalidNameError),
    InvalidBuildError(api::InvalidBuildError),

    // Storage Errors
    PackageNotFoundError(api::Ident),
    VersionExistsError(api::Ident),

    // Build Errors
    Collection(build::CollectionError),
    Build(build::BuildError),

    // Test Errors
    Test(test::TestError),

    /// Not running under an active spk environment
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

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Error {
        Error::IO(err)
    }
}

impl From<spfs::Error> for Error {
    fn from(err: spfs::Error) -> Error {
        Error::SPFS(err)
    }
}

impl From<serde_yaml::Error> for Error {
    fn from(err: serde_yaml::Error) -> Error {
        Error::Serde(err)
    }
}

impl From<solve::graph::GraphError> for Error {
    fn from(err: solve::graph::GraphError) -> Error {
        Error::Solve(err.into())
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(&format!("{:?}", self))
    }
}
