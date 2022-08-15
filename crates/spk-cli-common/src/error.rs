// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use colored::Colorize;
use spk_format::FormatError;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    IO(#[from] std::io::Error),
    #[error(transparent)]
    SPFS(#[from] spfs::Error),
    #[error("Error: {0}")]
    String(String),

    #[error(transparent)]
    SpkBuildError(#[from] spk_build::Error),
    #[error(transparent)]
    SpkExecError(#[from] spk_exec::Error),
    #[error(transparent)]
    SpkIdentError(#[from] spk_ident::Error),
    #[error(transparent)]
    SpkSolverError(#[from] spk_solver::Error),
    #[error(transparent)]
    SpkSpecError(#[from] spk_spec::Error),
    #[error(transparent)]
    SpkStorageError(#[from] spk_storage::Error),

    // Bake Errors
    #[error("Skip embedded")]
    SkipEmbedded,

    // Test Errors
    #[error(transparent)]
    Test(#[from] TestError),

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

impl FormatError for Error {
    fn format_error(&self, verbosity: u32) -> String {
        let mut msg = String::new();
        match self {
            /*
            Error::PackageNotFoundError(pkg) => {
                msg.push_str("Package not found: ");
                msg.push_str(&pkg.format_ident());
                msg.push('\n');
                msg.push_str(
                    &" * check the spelling of the name\n"
                        .yellow()
                        .dimmed()
                        .to_string(),
                );
                msg.push_str(
                    &" * ensure that you have enabled the right repositories"
                        .yellow()
                        .dimmed()
                        .to_string(),
                )
            }
            */
            Error::SpkSolverError(err) => return err.format_error(verbosity),
            Error::String(err) => msg.push_str(err),
            err => msg.push_str(&err.to_string()),
        }
        msg.red().to_string()
    }
}

/// Denotes that a test has failed or was invalid.
#[derive(Debug, Error)]
#[error("Test error: {message}")]
pub struct TestError {
    pub message: String,
}

impl TestError {
    pub fn new_error(msg: String) -> Error {
        Error::Test(Self { message: msg })
    }
}
