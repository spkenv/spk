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

pub type CloneableResult<T> = std::result::Result<T, CloneableError>;

#[derive(Clone, Debug)]
pub enum CloneableError {
    PackageNotFoundError(api::Ident),
    SpfsUnknownReference(String),
}

impl From<CloneableError> for Error {
    fn from(ce: CloneableError) -> Self {
        match ce {
            CloneableError::PackageNotFoundError(e) => Error::PackageNotFoundError(e),
            CloneableError::SpfsUnknownReference(e) => {
                Error::SPFS(spfs::Error::UnknownReference(e))
            }
        }
    }
}

impl From<&Error> for CloneableError {
    fn from(e: &Error) -> Self {
        match e {
            Error::IO(_) => todo!(),
            Error::SPFS(_) => todo!(),
            Error::Serde(_) => todo!(),
            Error::SkipEmbedded => todo!(),
            Error::Solve(_) => todo!(),
            Error::String(_) => todo!(),
            Error::InvalidVersionError(_) => todo!(),
            Error::InvalidNameError(_) => todo!(),
            Error::InvalidBuildError(_) => todo!(),
            Error::PackageNotFoundError(e) => CloneableError::PackageNotFoundError(e.clone()),
            Error::VersionExistsError(_) => todo!(),
            Error::Collection(_) => todo!(),
            Error::Build(_) => todo!(),
            Error::Test(_) => todo!(),
            Error::NoEnvironment => todo!(),
        }
    }
}

impl From<&spfs::Error> for CloneableError {
    fn from(e: &spfs::Error) -> Self {
        match e {
            spfs::Error::String(_) => todo!(),
            spfs::Error::Nix(_) => todo!(),
            spfs::Error::IO(_) => todo!(),
            spfs::Error::Errno(_, _) => todo!(),
            spfs::Error::JSON(_) => todo!(),
            spfs::Error::Config(_) => todo!(),
            spfs::Error::InvalidRemoteUrl(_) => todo!(),
            spfs::Error::InvalidDateTime(_) => todo!(),
            spfs::Error::Caps(_) => todo!(),
            spfs::Error::Utf8Error(_) => todo!(),
            spfs::Error::Tonic(_) => todo!(),
            spfs::Error::TokioJoinError(_) => todo!(),
            spfs::Error::UnknownObject(_) => todo!(),
            spfs::Error::UnknownReference(e) => CloneableError::SpfsUnknownReference(e.clone()),
            spfs::Error::AmbiguousReference(_) => todo!(),
            spfs::Error::InvalidReference(_) => todo!(),
            spfs::Error::NoRenderStorage(_) => todo!(),
            spfs::Error::FailedToOpenRepository { .. } => todo!(),
            spfs::Error::UnknownRemoteName(_) => todo!(),
            spfs::Error::NothingToCommit => todo!(),
            spfs::Error::NoActiveRuntime => todo!(),
            spfs::Error::RuntimeNotInitialized(_) => todo!(),
            spfs::Error::UnknownRuntime { .. } => todo!(),
            spfs::Error::RuntimeExists(_) => todo!(),
            spfs::Error::RuntimeAlreadyEditable => todo!(),
            spfs::Error::MissingBinary(_) => todo!(),
            spfs::Error::NoSupportedShell => todo!(),
            spfs::Error::IncompleteClean { .. } => todo!(),
        }
    }
}
