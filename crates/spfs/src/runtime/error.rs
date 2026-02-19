// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::io;

use miette::Diagnostic;
use thiserror::Error;

use crate::error::OsError;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Diagnostic, Debug, Error)]
#[diagnostic(
    url(
        "https://spkenv.dev/error_codes#{}",
        self.code().unwrap_or_else(|| Box::new("spfs::runtime"))
    )
)]
pub enum Error {
    #[error("Nothing to commit, resulting filesystem would be empty")]
    NothingToCommit,
    #[error("No active runtime")]
    NoActiveRuntime,
    #[error("Runtime has not been initialized: {0}")]
    RuntimeNotInitialized(String),
    #[error("Runtime does not exist: {runtime}")]
    UnknownRuntime {
        runtime: String,
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    #[error("Runtime already exists: {0}")]
    RuntimeExists(String),
    #[error(
        "An existing runtime is using the same upper name ({upper_name}).\nTry another name, or connect to the runtime by running:\n\n   spfs join {runtime_name} <command>"
    )]
    RuntimeUpperDirAlreadyInUse {
        upper_name: String,
        runtime_name: String,
    },
    #[error(
        "This kind of repository does not support durable runtime paths. A FSRepository is required for that."
    )]
    DoesNotSupportDurableRuntimePath,
    #[error("Runtime is already editable")]
    RuntimeAlreadyEditable,
    #[error("Runtime read error: {0}")]
    RuntimeReadError(std::path::PathBuf, #[source] io::Error),
    #[error("Runtime write error: {0}")]
    RuntimeWriteError(std::path::PathBuf, #[source] io::Error),
    #[error("Runtime set permissions error: {0}")]
    RuntimeSetPermissionsError(std::path::PathBuf, #[source] io::Error),
    #[error("Failed to create {} directory", crate::env::SPFS_DIR)]
    #[diagnostic(
        code("spfs::could_not_create_spfs_dir"),
        help("If you have sudo/admin privileges, you can try creating it yourself")
    )]
    CouldNotCreateSpfsRoot { source: std::io::Error },
    #[error("Unable to make the runtime durable: {0}")]
    RuntimeChangeToDurableError(String),
}

impl OsError for Error {
    fn os_error(&self) -> Option<i32> {
        match self {
            Error::RuntimeReadError(_, err) => err.os_error(),
            Error::RuntimeWriteError(_, err) => err.os_error(),
            _ => None,
        }
    }
}
