// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use colored::Colorize;
use spk_foundation::format::FormatError;
use spk_ident::PkgRequest;
use spk_solver_graph::Note;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    OutOfOptions(#[from] OutOfOptions),
    #[error("Solver interrupted: {0}")]
    SolverInterrupted(String),
    #[error(transparent)]
    SpkIdentComponentError(#[from] spk_foundation::ident_component::Error),
    #[error(transparent)]
    SpkSolverGraphError(#[from] spk_solver_graph::Error),
    #[error(transparent)]
    SpkSolverGraphGraphError(#[from] spk_solver_graph::GraphError),
    #[error(transparent)]
    SpkSolverPackageIteratorError(#[from] spk_solver_package_iterator::Error),
    #[error(transparent)]
    SpkSolverSolutionError(#[from] spk_solver_solution::Error),
    #[error(transparent)]
    SpkSolverValidationError(#[from] spk_solver_validation::Error),
    #[error(transparent)]
    SpkSpecError(#[from] spk_spec::Error),
    #[error("Error: {0}")]
    String(String),
}

#[derive(Debug, Error)]
#[error("Out of options for {pkg}", pkg = .request.pkg)]
pub struct OutOfOptions {
    pub request: PkgRequest,
    pub notes: Vec<Note>,
}

impl FormatError for Error {
    fn format_error(&self, verbosity: u32) -> String {
        let mut msg = String::new();
        msg.push_str("Failed to resolve");
        match self {
            Error::OutOfOptions(_) => {
                msg.push_str("\n * out of options");
            }
            Error::SolverInterrupted(err) => {
                msg.push_str("\n * ");
                msg.push_str(err.as_str());
            }
            Error::SpkSolverGraphError(err) => return err.format_error(verbosity),
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
