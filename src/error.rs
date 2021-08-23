// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use pyo3::{exceptions, prelude::*};

pub type Result<T> = std::result::Result<T, SpkError>;

#[derive(thiserror::Error, Debug)]
pub enum SpkError {
    #[error("Failed to write sources script to bash stdin: {0}")]
    ScriptSourceIo(#[from] std::io::Error),
    #[error("Failed to execute sources script: exit code {0:?}")]
    ScriptSourceExec(Option<i32>),
    #[error(transparent)]
    Spfs(#[from] spfs::Error),
    #[error(transparent)]
    Serde(#[from] serde_yaml::Error),
    #[error(transparent)]
    PyErr(#[from] PyErr),
    #[error("Collection: {0}")]
    Collection(String),
    #[error("Build: {0}")]
    Build(String),
    #[error("String: {0}")]
    String(String), // TODO: To general

    #[error("InstallSpec: {0}")]
    InstallSpec(String),

    // API Errors
    #[error("InvalidVersion: {0}")]
    InvalidVersion(String),
    #[error("InvalidVersionTag: {0}")]
    InvalidVersionTag(String),
    #[error("InvalidPackageName: {0}")]
    InvalidPackageName(String),
    #[error("InvalidBuildDigest: {0}")]
    InvalidBuildDigest(String),
    #[error("Too many tokens in range identifier")]
    InvalidRangeIdent,
    #[error("Invalid value '{value}' for option '{option}': {message}")]
    InvalidPkgOption {
        value: String,
        option: String,
        message: String,
    },
}

impl SpkError {
    /// Wraps an error message with a prefix, creating a contextual but generic error
    pub fn wrap<S: AsRef<str>>(prefix: S, err: Self) -> Self {
        // preserve PyErr types
        match err {
            SpkError::PyErr(pyerr) => SpkError::PyErr(Python::with_gil(|py| {
                PyErr::from_type(
                    pyerr.ptype(py),
                    format!("{}: {}", prefix.as_ref(), pyerr.pvalue(py).to_string()),
                )
            })),
            err => SpkError::String(format!("{}: {}", prefix.as_ref(), err)),
        }
    }
}

impl From<SpkError> for PyErr {
    fn from(err: SpkError) -> PyErr {
        use SpkError::*;
        match err {
            Spfs(err) => exceptions::PyRuntimeError::new_err(spfs::io::format_error(&err)),

            Serde(_) | Build(_) | Collection(_) | String(_) | InstallSpec(_)
            | ScriptSourceIo(_) | ScriptSourceExec(_) => {
                exceptions::PyRuntimeError::new_err(err.to_string())
            }

            InvalidPackageName(_)
            | InvalidVersion(_)
            | InvalidVersionTag(_)
            | InvalidBuildDigest(_)
            | InvalidRangeIdent { .. }
            | InvalidPkgOption { .. } => exceptions::PyValueError::new_err(err.to_string()),
            PyErr(err) => err,
        }
    }
}
