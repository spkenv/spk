// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::io;
use std::str::Utf8Error;

use miette::Diagnostic;
use thiserror::Error;

use crate::{encoding, storage};

#[derive(Diagnostic, Debug, Error)]
#[diagnostic(
    url(
        "https://getspk.io/error_codes#{}",
        self.code().unwrap_or_else(|| Box::new("spfs::generic"))
    )
)]
pub enum Error {
    #[error("{0}")]
    #[diagnostic(code("spfs::generic"))]
    String(String),
    #[cfg(unix)]
    #[error(transparent)]
    #[diagnostic(code("spfs::generic"))]
    Nix(#[from] nix::Error),
    #[cfg(windows)]
    #[error(transparent)]
    #[diagnostic(code("spfs::generic"))]
    Win(#[from] windows::core::Error),
    #[error("[ERRNO {1}] {0}")]
    #[diagnostic(code("spfs::generic"))]
    Errno(String, i32),
    #[error(transparent)]
    JSON(#[from] serde_json::Error),
    #[error(transparent)]
    YAML(#[from] serde_yaml::Error),
    #[error(transparent)]
    Config(#[from] config::ConfigError),

    #[error(transparent)]
    #[diagnostic(forward(0))]
    Encoding(#[from] super::encoding::Error),

    #[error("Invalid repository url: {0:?}")]
    InvalidRemoteUrl(#[from] url::ParseError),
    #[error("Invalid date time: {0:?}")]
    InvalidDateTime(#[from] chrono::ParseError),
    #[error("time specifier is not valid: {given} ({reason})")]
    InvalidTimeSpec { given: String, reason: String },
    #[error("Invalid path {0}")]
    InvalidPath(std::path::PathBuf, #[source] io::Error),
    #[cfg(unix)]
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
    #[error("Cannot write to a repository which has been pinned in time")]
    RepositoryIsPinned,

    #[error("Failed to open repository: {repository}")]
    #[diagnostic(code("spfs::failed_to_open_repo"))]
    FailedToOpenRepository {
        repository: String,
        #[diagnostic_source]
        #[source]
        source: storage::OpenRepositoryError,
    },

    #[error("No remote named '{0}' configured")]
    #[diagnostic(
        code("spfs::unknown_remote"),
        help("See available remotes via the 'spfs config' command")
    )]
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
    #[error("An existing runtime is using the same upper name ({upper_name}).\nTry another name, or connect to the runtime by running:\n\n   spfs join {runtime_name} <command>")]
    RuntimeUpperDirAlreadyInUse {
        upper_name: String,
        runtime_name: String,
    },
    #[error("This kind of repository does not support durable runtime paths. A FSRepository is required for that.")]
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

    #[cfg(unix)]
    #[error("OverlayFS kernel module does not appear to be installed")]
    OverlayFsNotInstalled,

    #[error("{}, and {} more errors during clean", errors.get(0).unwrap(), errors.len() - 1)]
    IncompleteClean { errors: Vec<Self> },

    #[error("OverlayFS mount backend is not supported on windows.")]
    OverlayFsUnsupportedOnWindows,

    #[error("{context}")]
    Wrapped {
        context: String,
        #[related]
        related: Vec<Self>,
        source: Box<Self>,
    },
}

impl Error {
    pub fn new<S: AsRef<str>>(message: S) -> Error {
        Error::new_errno(libc::EINVAL, message.as_ref())
    }

    pub fn new_errno<E: Into<String>>(errno: i32, e: E) -> Error {
        let msg = e.into();
        Error::Errno(msg, errno)
    }

    #[cfg(unix)]
    pub fn wrap_nix<E: Into<String>>(err: nix::Error, prefix: E) -> Error {
        let err = Self::from(err);
        err.wrap(prefix)
    }

    /// Wrap this error, adding an additional level of context
    pub fn wrap<E: Into<String>>(self, msg: E) -> Error {
        match self {
            Self::Wrapped {
                mut context,
                mut related,
                source,
            } => {
                related.push(Self::String(context));
                context = msg.into();
                Self::Wrapped {
                    context,
                    related,
                    source,
                }
            }
            source => {
                let context = msg.into();
                Self::Wrapped {
                    context,
                    related: Vec::new(),
                    source: Box::new(source),
                }
            }
        }
    }

    /// Create an [`Error::FailedToOpenRepository`] instance for
    /// a repository using its address and root cause.
    pub fn failed_to_open_repository<R: storage::Repository>(
        repo: &R,
        source: storage::OpenRepositoryError,
    ) -> Self {
        Self::FailedToOpenRepository {
            repository: repo.address().to_string(),
            source,
        }
    }

    /// Create an [`Error::ProcessSpawnError`] with context.
    pub fn process_spawn_error<S>(
        process_description: S,
        err: std::io::Error,
        current_dir: Option<std::path::PathBuf>,
    ) -> Error
    where
        S: std::fmt::Display + Into<String>,
    {
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
        Error::ProcessSpawnError(process_description.into(), err)
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

// impl IntoError for storage::OpenRepositoryError {
//     type Context = url::Url;
//     type Error = Error;

//     fn into_error(self, context: Self::Context) -> Self::Error {
//         Error::FailedToOpenRepository {
//             repository: context.into(),
//             source: self,
//         }
//     }
// }

// /// A type that can be converted into an error if
// /// some additional context is provided
// pub trait IntoError {
//     type Context;
//     type Error;

//     fn into_error(self, context: Self::Context) -> Self::Error;
// }

/// An OS error represents an error that may have an associated
/// error code from the operating system
pub trait OsError {
    /// The underlying os error code for this error, if any
    //
    // TODO: make this function unsafe to encourage all code to be
    // platform-agnostic and have specific is_* functions for the cases
    // that our codebase would like to handle
    fn os_error(&self) -> Option<i32>;
}

/// An extension trait for [`OsError`]s that provide platform-agnostic
/// functions for determining the abstract cause of an error
pub trait OsErrorExt: OsError {
    /// True if the root cause of this error was that a file or directory
    /// did not exist in the underlying OS filesystem
    fn is_os_not_found(&self) -> bool {
        #[cfg(windows)]
        const NOT_FOUND: &[i32] = &[
            windows::Win32::Foundation::ERROR_PATH_NOT_FOUND.0 as i32,
            windows::Win32::Foundation::ERROR_FILE_NOT_FOUND.0 as i32,
        ];
        match self.os_error() {
            #[cfg(windows)]
            Some(c) if NOT_FOUND.contains(&c) => true,
            #[cfg(unix)]
            Some(libc::ENOENT) => true,
            _ => false,
        }
    }
}

// this blanket implementation intentionally stops anyone
// from redefining functions from the ext trait
impl<T: ?Sized> OsErrorExt for T where T: OsError {}

impl OsError for Error {
    fn os_error(&self) -> Option<i32> {
        match self {
            #[cfg(unix)]
            Error::UnknownObject(_) => Some(libc::ENOENT),
            #[cfg(windows)]
            Error::UnknownObject(_) => {
                Some(windows::Win32::Foundation::ERROR_FILE_NOT_FOUND.0 as i32)
            }
            Error::Encoding(encoding::Error::FailedRead(err)) => err.os_error(),
            Error::Encoding(encoding::Error::FailedWrite(err)) => err.os_error(),
            Error::ProcessSpawnError(_, err) => err.os_error(),
            Error::RuntimeReadError(_, err) => err.os_error(),
            Error::RuntimeWriteError(_, err) => err.os_error(),
            Error::StorageReadError(_, _, err) => err.os_error(),
            Error::StorageWriteError(_, _, err) => err.os_error(),
            Error::Errno(_, errno) => Some(*errno),
            #[cfg(unix)]
            Error::Nix(err) => Some(*err as i32),
            _ => None,
        }
    }
}

impl OsError for std::io::Error {
    fn os_error(&self) -> Option<i32> {
        match self.raw_os_error() {
            Some(errno) => Some(errno),
            None => match self.kind() {
                #[cfg(unix)]
                std::io::ErrorKind::UnexpectedEof => Some(libc::EOF),
                #[cfg(windows)]
                std::io::ErrorKind::UnexpectedEof => {
                    Some(windows::Win32::Foundation::ERROR_HANDLE_EOF.0 as i32)
                }
                _ => None,
            },
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;
