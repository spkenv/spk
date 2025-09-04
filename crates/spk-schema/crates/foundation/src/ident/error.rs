// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use miette::Diagnostic;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Diagnostic, Debug, Error)]
#[diagnostic(
    url(
        "https://spkenv.dev/error_codes#{}",
        self.code().unwrap_or_else(|| Box::new("spk::generic"))
    )
)]
pub enum Error {
    #[error("Invalid PreReleasePolicy: {0}")]
    InvalidPreReleasePolicy(#[source] serde_yaml::Error),
    #[error("Invalid InclusionPolicy: {0}")]
    InvalidInclusionPolicy(#[source] serde_yaml::Error),
    #[error("Invalid PinPolicy: {0}")]
    InvalidPinPolicy(#[source] serde_yaml::Error),
    #[error(transparent)]
    #[diagnostic(forward(0))]
    SpkIdentBuildError(#[from] crate::ident_build::Error),
    #[error(transparent)]
    #[diagnostic(forward(0))]
    SpkNameError(#[from] crate::name::Error),
    #[error(transparent)]
    #[diagnostic(forward(0))]
    SpkVersionError(#[from] crate::version::Error),
    #[error(transparent)]
    #[diagnostic(forward(0))]
    SpkVersionRangeError(#[from] crate::version_range::Error),
    #[error("Error: {0}")]
    String(String),
}

impl From<&str> for Error {
    fn from(err: &str) -> Error {
        Error::String(err.to_owned())
    }
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
