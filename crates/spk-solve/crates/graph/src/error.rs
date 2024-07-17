// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::fmt::Write;

use colored::Colorize;
use miette::Diagnostic;
use spk_schema::foundation::format::FormatError;
use spk_schema::ident::PkgRequest;
use thiserror::Error;

use super::graph::GraphError;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Diagnostic, Debug, Error)]
#[diagnostic(
    url(
        "https://spkenv.dev/error_codes#{}",
        self.code().unwrap_or_else(|| Box::new("spk::generic"))
    )
)]
pub enum Error {
    #[error(transparent)]
    #[diagnostic(forward(0))]
    FailedToResolve(#[from] super::graph::Graph),
    #[error(transparent)]
    #[diagnostic(forward(0))]
    Graph(#[from] GraphError),
    #[error("Package not found: {0}")]
    PackageNotFoundDuringSolve(PkgRequest),
    #[error("Solver error: {0}")]
    SolverError(String),
    #[error("Solver interrupted: {0}")]
    SolverInterrupted(String),
    #[error(transparent)]
    #[diagnostic(forward(0))]
    SpkIdentError(#[from] spk_schema::ident::Error),
    #[error(transparent)]
    #[diagnostic(forward(0))]
    SpkIdentComponentError(#[from] spk_schema::foundation::ident_component::Error),
    #[error(transparent)]
    #[diagnostic(forward(0))]
    SpkNameError(#[from] spk_schema::foundation::name::Error),
    #[error(transparent)]
    #[diagnostic(forward(0))]
    SpkSolverPackageIteratorError(#[from] spk_solve_package_iterator::Error),
    #[error(transparent)]
    #[diagnostic(forward(0))]
    SpkSolverSolutionError(#[from] spk_solve_solution::Error),
    #[error(transparent)]
    #[diagnostic(forward(0))]
    SpkSpecError(#[from] spk_schema::Error),
    #[error(transparent)]
    #[diagnostic(forward(0))]
    SpkStorageError(#[from] spk_storage::Error),
    #[error(transparent)]
    #[diagnostic(forward(0))]
    SpkVersionRangeError(#[from] spk_schema::foundation::version_range::Error),
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

#[derive(Diagnostic, Debug, Error)]
pub enum GetMergedRequestError {
    #[error("No request for: {0}")]
    NoRequestFor(String),
    #[error(transparent)]
    #[diagnostic(forward(0))]
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
    fn format_error(&self, verbosity: u8) -> String {
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
