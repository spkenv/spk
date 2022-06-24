// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use thiserror::Error;

use crate::api;

use super::graph::{GraphError, Note};

#[derive(Debug, Error)]
pub enum Error {
    #[error("Solver error: {0}")]
    SolverError(String),
    #[error(transparent)]
    FailedToResolve(#[from] super::Graph),
    #[error(transparent)]
    Graph(#[from] GraphError),
    #[error(transparent)]
    OutOfOptions(#[from] OutOfOptions),
    #[error("Solver interrupted: {0}")]
    SolverInterrupted(String),
    #[error("Package not found: {0}")]
    PackageNotFoundDuringSolve(#[from] api::PkgRequest),
}

pub type GetCurrentResolveResult<T> = std::result::Result<T, GetCurrentResolveError>;

#[derive(Debug, Error)]
pub enum GetCurrentResolveError {
    #[error("Package not resolved: {0}")]
    PackageNotResolved(String),
}

pub type GetMergedRequestResult<T> = std::result::Result<T, GetMergedRequestError>;

#[derive(Debug, Error)]
pub enum GetMergedRequestError {
    #[error("No request for: {0}")]
    NoRequestFor(String),
    #[error(transparent)]
    Other(#[from] Box<crate::Error>),
}

impl From<GetMergedRequestError> for crate::Error {
    fn from(err: GetMergedRequestError) -> Self {
        match err {
            GetMergedRequestError::NoRequestFor(s) => crate::Error::String(s),
            GetMergedRequestError::Other(err) => *err,
        }
    }
}

impl From<crate::Error> for GetMergedRequestError {
    fn from(err: crate::Error) -> Self {
        GetMergedRequestError::Other(Box::new(err))
    }
}

#[derive(Debug, Error)]
#[error("Out of options for {pkg}", pkg = .request.pkg)]
pub struct OutOfOptions {
    pub request: api::PkgRequest,
    pub notes: Vec<Note>,
}
