// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use crate::api;

use super::graph::{GraphError, Note};

#[derive(Debug)]
pub enum Error {
    SolverError(String),
    FailedToResolve(super::Graph),
    Graph(GraphError),
    OutOfOptions(OutOfOptions),
    SolverInterrupted(String),
}

impl From<Error> for crate::Error {
    fn from(err: Error) -> Self {
        crate::Error::Solve(err)
    }
}

impl From<GraphError> for Error {
    fn from(err: GraphError) -> Self {
        Error::Graph(err)
    }
}

pub type GetCurrentResolveResult<T> = std::result::Result<T, GetCurrentResolveError>;

pub enum GetCurrentResolveError {
    PackageNotResolved(String),
}

pub type GetMergedRequestResult<T> = std::result::Result<T, GetMergedRequestError>;

#[derive(Debug)]
pub enum GetMergedRequestError {
    NoRequestFor(String),
    Other(Box<crate::Error>),
}

impl std::fmt::Display for GetMergedRequestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoRequestFor(s) => s.fmt(f),
            Self::Other(s) => s.fmt(f),
        }
    }
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

#[derive(Debug)]
pub struct OutOfOptions {
    pub request: api::PkgRequest,
    pub notes: Vec<Note>,
}

impl std::fmt::Display for OutOfOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Out of options")
    }
}
