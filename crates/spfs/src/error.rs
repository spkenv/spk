// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::io;

use thiserror::Error;

use crate::encoding;

#[derive(Debug, Error)]
pub enum Error {
    #[error("{0}")]
    String(String),
    #[error(transparent)]
    Nix(#[from] nix::Error),
    #[error(transparent)]
    IO(#[from] io::Error),
    #[error("[ERRNO {1}] {0}")]
    Errno(String, i32),
    #[error(transparent)]
    JSON(#[from] serde_json::Error),
    #[error(transparent)]
    Config(#[from] config::ConfigError),

    /// Denotes a missing object or one that is not present in the database.
    #[error("Unknown Object: {0}")]
    UnknownObject(encoding::Digest),
    /// Denotes a reference that is not present in the database
    #[error("Unknown Reference: {0}")]
    UnknownReference(String),
    /// Denotes a reference that could refer to more than one object in the storage.
    #[error("Ambiguous reference [too short]: {0}")]
    AmbiguousReference(String),
    /// Denotes a reference that does not meet the syntax requirements
    #[error("Invalid Reference: {0}")]
    InvalidReference(String),

    #[error("Nothing to commit, resulting filesystem would be empty")]
    NothingToCommit,
    #[error("No active runtime")]
    NoRuntime,

    #[error("'{0}' not found in PATH, was it installed properly?")]
    MissingBinary(&'static str),
    #[error("No supported shell found, or no support for current shell")]
    NoSupportedShell,
}

impl Error {
    pub fn new<S: AsRef<str>>(message: S) -> Error {
        Error::new_errno(libc::EINVAL, message.as_ref())
    }

    pub fn new_errno<E: Into<String>>(errno: i32, e: E) -> Error {
        let msg = e.into();
        Error::Errno(msg, errno)
    }

    pub fn wrap_io<E: Into<String>>(err: std::io::Error, prefix: E) -> Error {
        let err = Self::from(err);
        err.wrap(prefix)
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
        match self {
            Error::IO(err) => match err.raw_os_error() {
                Some(errno) => Some(errno),
                None => match err.kind() {
                    std::io::ErrorKind::UnexpectedEof => Some(libc::EOF),
                    _ => None,
                },
            },
            Error::Errno(_, errno) => Some(*errno),
            Error::Nix(err) => {
                let errno = err.as_errno();
                if let Some(e) = errno {
                    return Some(e as i32);
                }
                None
            }
            _ => None,
        }
    }
}

impl From<nix::errno::Errno> for Error {
    fn from(errno: nix::errno::Errno) -> Error {
        Error::Nix(nix::Error::from_errno(errno))
    }
}
impl From<i32> for Error {
    fn from(errno: i32) -> Error {
        Error::IO(std::io::Error::from_raw_os_error(errno))
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

impl From<walkdir::Error> for Error {
    fn from(err: walkdir::Error) -> Self {
        let msg = err.to_string();
        match err.into_io_error() {
            Some(err) => err.into(),
            None => Self::String(msg),
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;
