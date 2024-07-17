// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use colored::Colorize;
use miette::Diagnostic;
use spk_schema::foundation::format::FormatError;
use spk_solve_graph::Graph;
use thiserror::Error;

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
    FailedToResolve(#[from] Graph),
    #[error("Solver error: {0}")]
    SolverError(String),
    #[error(transparent)]
    #[diagnostic(forward(0))]
    SpkIdentError(#[from] spk_schema::ident::Error),
    #[error(transparent)]
    #[diagnostic(forward(0))]
    SpkSolverGraphGetMergedRequestError(#[from] spk_solve_graph::GetMergedRequestError),
    #[error(transparent)]
    #[diagnostic(forward(0))]
    SpkStorageError(#[from] spk_storage::Error),
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
