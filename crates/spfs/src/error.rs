// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::io;

use thiserror::Error;

use super::commit::NothingToCommitError;
use super::status::NoRuntimeError;
use crate::graph;

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

    #[error("{0}")]
    UnknownObject(#[from] graph::UnknownObjectError),
    #[error("{0}")]
    UnknownReference(#[from] graph::UnknownReferenceError),
    #[error("{0}")]
    AmbiguousReference(#[from] graph::AmbiguousReferenceError),
    #[error("{0}")]
    InvalidReference(#[from] graph::InvalidReferenceError),
    #[error("{0}")]
    NothingToCommit(#[from] NothingToCommitError),
    #[error("{0}")]
    NoRuntime(#[from] NoRuntimeError),
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
