// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use colored::Colorize;
use miette::Diagnostic;
use spk_schema::foundation::format::FormatError;
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
    #[error(transparent)]
    #[diagnostic(forward(0))]
    Error(#[from] spfs::Error),
    #[error("Error: {0}")]
    String(String),

    #[error(transparent)]
    #[diagnostic(forward(0))]
    SpkBuildError(#[from] spk_build::Error),
    #[error(transparent)]
    #[diagnostic(forward(0))]
    SpkExecError(#[from] spk_exec::Error),
    #[error(transparent)]
    #[diagnostic(forward(0))]
    SpkIdentError(#[from] spk_schema::ident::Error),
    #[error(transparent)]
    #[diagnostic(forward(0))]
    SpkSolverError(#[from] spk_solve::Error),
    #[error(transparent)]
    #[diagnostic(forward(0))]
    SpkSpecError(#[from] spk_schema::Error),
    #[error(transparent)]
    #[diagnostic(forward(0))]
    SpkStorageError(#[from] spk_storage::Error),

    // IO Errors
    #[error("Failed to write file {0}")]
    FileWriteError(std::path::PathBuf, #[source] std::io::Error),
    #[error(transparent)]
    #[diagnostic(forward(0))]
    ProcessSpawnError(spfs::Error),
    #[error("Failed to create temp dir: {0}")]
    TempDirError(#[source] std::io::Error),

    // Test Errors
    #[error(transparent)]
    #[diagnostic(forward(0))]
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
    fn format_error(&self, verbosity: u8) -> String {
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
#[derive(Debug, Diagnostic, Error)]
#[error("Test error: {message}")]
pub struct TestError {
    pub message: String,
}

impl TestError {
    pub fn new_error(msg: String) -> Error {
        Error::Test(Self { message: msg })
    }
}
