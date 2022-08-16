// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::fmt::Write;

use colored::Colorize;
use spk_foundation::format::FormatError;
use spk_ident::PkgRequest;
use thiserror::Error;

use super::graph::GraphError;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    FailedToResolve(#[from] super::graph::Graph),
    #[error(transparent)]
    Graph(#[from] GraphError),
    #[error("Package not found: {0}")]
    PackageNotFoundDuringSolve(#[from] PkgRequest),
    #[error("Solver error: {0}")]
    SolverError(String),
    #[error("Solver interrupted: {0}")]
    SolverInterrupted(String),
    #[error(transparent)]
    SpkIdentError(#[from] spk_ident::Error),
    #[error(transparent)]
    SpkIdentComponentError(#[from] spk_foundation::ident_component::Error),
    #[error(transparent)]
    SpkNameError(#[from] spk_name::Error),
    #[error(transparent)]
    SpkSolverPackageIteratorError(#[from] spk_solver_package_iterator::Error),
    #[error(transparent)]
    SpkSolverSolutionError(#[from] spk_solver_solution::Error),
    #[error(transparent)]
    SpkSpecError(#[from] spk_spec::Error),
    #[error(transparent)]
    SpkStorageError(#[from] spk_storage::Error),
    #[error(transparent)]
    SpkValidatorsError(#[from] spk_validators::Error),
    #[error(transparent)]
    SpkVersionRangeError(#[from] spk_foundation::version_range::Error),
    #[error("Error: {0}")]
    String(String),
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

impl FormatError for Error {
    fn format_error(&self, verbosity: u32) -> String {
        let mut msg = String::new();
        msg.push_str("Failed to resolve");
        match self {
            Error::FailedToResolve(_graph) => {
                // TODO: provide a summary based on the graph
            }
            Error::SolverError(reason) => {
                msg.push_str("\n * ");
                msg.push_str(reason.as_str());
            }
            Error::Graph(err) => {
                msg.push_str("\n * ");
                msg.push_str(&err.to_string());
            }
            Error::SolverInterrupted(err) => {
                msg.push_str("\n * ");
                msg.push_str(err.as_str());
            }
            Error::PackageNotFoundDuringSolve(request) => {
                let requirers: Vec<String> = request
                    .get_requesters()
                    .iter()
                    .map(ToString::to_string)
                    .collect();
                msg.push_str("\n * ");
                let _ = write!(msg, "Package '{}' not found during the solve as required by: {}.\n   Please check the package name's spelling", request.pkg, requirers.join(", "));
            }
            err => {
                msg.push_str("\n * ");
                msg.push_str(err.to_string().as_str());
            }
        }
        match verbosity {
            0 => {
                msg.push_str(&"\n * try '--verbose/-v' for more info".dimmed().yellow());
            }
            1 => {
                msg.push_str(&"\n * try '-vv' for even more info".dimmed().yellow());
            }
            2 => {
                msg.push_str(&"\n * try '-vvv' for even more info".dimmed().yellow());
            }
            3.. => (),
        }
        msg
    }
}
