// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use pyo3::{exceptions, prelude::*};
use spfs;

use super::api;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    IO(std::io::Error),
    SPFS(spfs::Error),
    Collection(crate::build::CollectionError),
    Build(crate::build::BuildError),
    String(String),

    // API Errors
    InvalidVersionError(api::InvalidVersionError),
    InvalidNameError(api::InvalidNameError),
    InvalidBuildError(api::InvalidBuildError),
}

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

impl From<Error> for PyErr {
    fn from(err: Error) -> PyErr {
        match err {
            Error::IO(err) => err.into(),
            Error::SPFS(spfs::Error::IO(err)) => err.into(),
            Error::SPFS(err) => exceptions::PyRuntimeError::new_err(spfs::io::format_error(&err)),
            Error::Build(err) => exceptions::PyRuntimeError::new_err(err.message.to_string()),
            Error::Collection(err) => exceptions::PyRuntimeError::new_err(err.message.to_string()),
            Error::String(msg) => exceptions::PyRuntimeError::new_err(msg.to_string()),
            Error::InvalidBuildError(err) => {
                exceptions::PyValueError::new_err(err.message.to_string())
            }
            Error::InvalidVersionError(err) => {
                exceptions::PyValueError::new_err(err.message.to_string())
            }
            Error::InvalidNameError(err) => {
                exceptions::PyValueError::new_err(err.message.to_string())
            }
        }
    }
}
