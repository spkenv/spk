// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::path::PathBuf;

use colored::Colorize;
use miette::Diagnostic;
use spk_schema::foundation::format::FormatError;
use spk_schema::ident::PkgRequest;
use spk_solve_graph::Note;
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
    OutOfOptions(#[from] OutOfOptions),
    #[error("Solver interrupted: {0}")]
    SolverInterrupted(String),
    #[error(transparent)]
    #[diagnostic(forward(0))]
    SpkIdentComponentError(#[from] spk_schema::foundation::ident_component::Error),
    #[error(transparent)]
    #[diagnostic(forward(0))]
    SpkIdentError(#[from] spk_schema::ident::Error),
    #[error(transparent)]
    #[diagnostic(forward(0))]
    GraphError(#[from] spk_solve_graph::Error),
    #[error(transparent)]
    #[diagnostic(forward(0))]
    GraphGraphError(#[from] spk_solve_graph::GraphError),
    #[error(transparent)]
    #[diagnostic(forward(0))]
    PackageIteratorError(#[from] spk_solve_package_iterator::Error),
    #[error(transparent)]
    #[diagnostic(forward(0))]
    SolutionError(#[from] spk_solve_solution::Error),
    #[error(transparent)]
    #[diagnostic(forward(0))]
    ValidationError(#[from] spk_solve_validation::Error),
    #[error(transparent)]
    #[diagnostic(forward(0))]
    SpkSpecError(#[from] spk_schema::Error),
    #[error("Status bar IO error: {0}")]
    StatusBarIOError(#[source] std::io::Error),
    #[error("Initial requests contain {0} impossible request{plural}.", plural = if *.0 == 1 { "" } else { "s" } )]
    InitialRequestsContainImpossibleError(usize),
    #[error(transparent)]
    #[diagnostic(forward(0))]
    SpkStorageError(#[from] spk_storage::Error),
    #[error("Error: {0}")]
    String(String),
    #[error("Error: {0} is not supported")]
    IncludingThisOutputNotSupported(String),
    #[error("Error: Solver log file not created: {1} - {0}")]
    SolverLogFileIOError(#[source] std::io::Error, PathBuf),
}

#[derive(Diagnostic, Debug, Error)]
#[error("Out of options for {pkg}", pkg = .request.pkg)]
pub struct OutOfOptions {
    pub request: PkgRequest,
    pub notes: Vec<Note>,
}

impl FormatError for Error {
    fn format_error(&self, verbosity: u8) -> String {
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
            Error::GraphError(err) => return err.format_error(verbosity),
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
