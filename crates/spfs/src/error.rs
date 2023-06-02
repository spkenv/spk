// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::io;
use std::str::Utf8Error;

use thiserror::Error;

use crate::encoding;

#[derive(Debug, Error)]
pub enum Error {
    #[error("{0}")]
    String(String),
    #[error(transparent)]
    Nix(#[from] nix::Error),
    #[error("[ERRNO {1}] {0}")]
    Errno(String, i32),
    #[error(transparent)]
    JSON(#[from] serde_json::Error),
    #[error(transparent)]
    Config(#[from] config::ConfigError),

    #[error(transparent)]
    Encoding(#[from] super::encoding::Error),

    #[error("Invalid repository url: {0:?}")]
    InvalidRemoteUrl(#[from] url::ParseError),
    #[error("Invalid date time: {0:?}")]
    InvalidDateTime(#[from] chrono::ParseError),
    #[error("Invalid path {0}")]
    InvalidPath(std::path::PathBuf, #[source] io::Error),
    #[error(transparent)]
    Caps(#[from] caps::errors::CapsError),
    #[error(transparent)]
    Utf8Error(#[from] Utf8Error),
    #[error("Error communicating with the server: {0:?}")]
    Tonic(#[from] tonic::Status),
    #[error(transparent)]
    TokioJoinError(#[from] tokio::task::JoinError),
    #[error("Failed to spawn {0}")]
    ProcessSpawnError(String, #[source] io::Error),

    /// Denotes a missing object or one that is not present in the database.
    #[error("Unknown Object: {0}")]
    UnknownObject(encoding::Digest),
    /// Denotes an object missing its payload.
    #[error("Object {0} missing payload: {1}")]
    ObjectMissingPayload(crate::graph::Object, encoding::Digest),
    /// Denotes a reference that is not present in the database
    #[error("Unknown Reference: {0}")]
    UnknownReference(String),
    /// Denotes a reference that could refer to more than one object in the storage.
    #[error("Ambiguous reference [too short]: {0}")]
    AmbiguousReference(String),
    /// Denotes a reference that does not meet the syntax requirements
    #[error("Invalid Reference: {0}")]
    InvalidReference(String),
    #[error("Repository does not support manifest rendering: {0:?}")]
    NoRenderStorage(url::Url),
    #[error("Object is not a blob: {1}")]
    ObjectNotABlob(crate::graph::Object, encoding::Digest),

    #[error("Failed to open repository: {repository}")]
    FailedToOpenRepository {
        repository: String,
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    #[error("No remote name '{0}' configured.")]
    UnknownRemoteName(String),

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
    #[error("Runtime is already editable")]
    RuntimeAlreadyEditable,
    #[error("Runtime read error: {0}")]
    RuntimeReadError(std::path::PathBuf, #[source] io::Error),
    #[error("Runtime write error: {0}")]
    RuntimeWriteError(std::path::PathBuf, #[source] io::Error),
    #[error("Runtime set permissions error: {0}")]
    RuntimeSetPermissionsError(std::path::PathBuf, #[source] io::Error),
    #[error("Storage read error from {0} at {1}: {2}")]
    StorageReadError(&'static str, std::path::PathBuf, #[source] io::Error),
    #[error("Storage write error from {0} at {1}: {2}")]
    StorageWriteError(&'static str, std::path::PathBuf, #[source] io::Error),

    #[error("'{0}' not found in PATH, was it installed properly?")]
    MissingBinary(&'static str),
    #[error("No supported shell found, or no support for current shell")]
    NoSupportedShell,
    #[error("Command, arguments or environment contained a nul byte, this is not supported")]
    CommandHasNul(#[source] std::ffi::NulError),

    #[error("{}, and {} more errors during clean", errors.get(0).unwrap(), errors.len() - 1)]
    IncompleteClean { errors: Vec<Self> },
}

impl Error {
    pub fn new<S: AsRef<str>>(message: S) -> Error {
        Error::new_errno(libc::EINVAL, message.as_ref())
    }

    pub fn new_errno<E: Into<String>>(errno: i32, e: E) -> Error {
        let msg = e.into();
        Error::Errno(msg, errno)
    }

    pub fn wrap_nix<E: Into<String>>(err: nix::Error, prefix: E) -> Error {
        let err = Self::from(err);
        err.wrap(prefix)
    }

    pub fn wrap<E: Into<String>>(&self, prefix: E) -> Error {
        let msg = format!("{}: {:?}", prefix.into(), self);
        match self.raw_os_error() {
            Some(errno) => Error::new_errno(errno, msg),
            None => Error::new(msg),
        }
    }

    pub fn raw_os_error(&self) -> Option<i32> {
        let handle_io_error = |err: &io::Error| match err.raw_os_error() {
            Some(errno) => Some(errno),
            None => match err.kind() {
                std::io::ErrorKind::UnexpectedEof => Some(libc::EOF),
                _ => None,
            },
        };

        match self {
            Error::UnknownObject(_) => Some(libc::ENOENT),
            Error::Encoding(encoding::Error::FailedRead(err)) => handle_io_error(err),
            Error::Encoding(encoding::Error::FailedWrite(err)) => handle_io_error(err),
            Error::ProcessSpawnError(_, err) => handle_io_error(err),
            Error::RuntimeReadError(_, err) => handle_io_error(err),
            Error::RuntimeWriteError(_, err) => handle_io_error(err),
            Error::StorageReadError(_, _, err) => handle_io_error(err),
            Error::StorageWriteError(_, _, err) => handle_io_error(err),
            Error::Errno(_, errno) => Some(*errno),
            Error::Nix(err) => Some(*err as i32),
            _ => None,
        }
    }

    /// Create an `Error:ProcessSpawnError` with context.
    pub fn process_spawn_error(
        process_description: String,
        err: std::io::Error,
        current_dir: Option<std::path::PathBuf>,
    ) -> Error {
        // A common problem with launching a sub-process is that the specified
        // current working directory doesn't exist.
        match (err.kind(), current_dir) {
            (std::io::ErrorKind::NotFound, Some(current_dir)) if !current_dir.exists() => {
                return Error::ProcessSpawnError(
                    format!(
                        "{process_description}: specified current_dir({current_dir}) doesn't exist",
                        current_dir = current_dir.display()
                    ),
                    err,
                );
            }
            _ => {}
        }
        Error::ProcessSpawnError(process_description, err)
    }
}

impl From<String> for Error {
    fn from(err: String) -> Self {
        Self::String(err)
    }
}
impl From<&str> for Error {
    fn from(err: &str) -> Self {
        Self::String(err.to_string())
    }
}
impl From<std::path::StripPrefixError> for Error {
    fn from(err: std::path::StripPrefixError) -> Self {
        Error::String(err.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
