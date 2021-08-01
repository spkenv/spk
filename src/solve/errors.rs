// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use pyo3::exceptions::PyException;
use pyo3::{create_exception, PyErr};

use crate::api;

use super::graph::NoteEnum;

create_exception!(errors, SolverError, PyException);

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    // SolverError(SolverError),
    OutOfOptions(OutOfOptions),
}

impl From<Error> for PyErr {
    fn from(err: Error) -> Self {
        match err {
            Error::OutOfOptions(err) => SolverError::new_err(err.to_string()),
        }
    }
}

#[derive(Debug)]
pub struct OutOfOptions {
    pub request: api::PkgRequest,
    pub notes: Vec<NoteEnum>,
}

impl std::fmt::Display for OutOfOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Out of options")
    }
}
