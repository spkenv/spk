// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use colored::Colorize;
use spk_foundation::format::FormatError;
use spk_solver_graph::Graph;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    FailedToResolve(#[from] Graph),
    #[error("Solver error: {0}")]
    SolverError(String),
    #[error(transparent)]
    SpkIdentError(#[from] spk_ident::Error),
    #[error(transparent)]
    SpkSolverGraphGetMergedRequestError(#[from] spk_solver_graph::GetMergedRequestError),
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
