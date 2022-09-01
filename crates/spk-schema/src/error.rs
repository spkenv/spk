// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::{fmt::Write, io::BufRead};

use thiserror::Error;

#[cfg(test)]
#[path = "./error_test.rs"]
mod error_test;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Failed to open file {0}")]
    FileOpenError(std::path::PathBuf, #[source] std::io::Error),
    #[error("Failed to write file {0}")]
    FileWriteError(std::path::PathBuf, #[source] std::io::Error),
    #[error("Invalid inheritance: {0}")]
    InvalidInheritance(#[source] serde_yaml::Error),
    #[error("Invalid package spec file {0}: {1}")]
    InvalidPackageSpecFile(std::path::PathBuf, #[source] serde_yaml::Error),
    #[error("Invalid path {0}")]
    InvalidPath(std::path::PathBuf, #[source] std::io::Error),
    #[error(transparent)]
    ProcessSpawnError(spfs::Error),
    #[error("Failed to wait for process: {0}")]
    ProcessWaitError(#[source] std::io::Error),
    #[error("Failed to encode spec: {0}")]
    SpecEncodingError(#[source] serde_yaml::Error),
    #[error(transparent)]
    SPFS(#[from] spfs::Error),
    #[error(transparent)]
    SpkIdentBuildError(#[from] crate::foundation::ident_build::Error),
    #[error(transparent)]
    SpkIdentComponentError(#[from] crate::foundation::ident_component::Error),
    #[error(transparent)]
    SpkIdentError(#[from] crate::ident::Error),
    #[error(transparent)]
    SpkNameError(#[from] crate::foundation::name::Error),
    #[error("Error: {0}")]
    String(String),
    #[error("Failed to create temp dir: {0}")]
    TempDirError(#[source] std::io::Error),

    #[error(transparent)]
    InvalidYaml(InvalidYamlError),
}

impl Error {
    /// Wraps an error message with a prefix, creating a contextual but generic error
    pub fn wrap<S: AsRef<str>>(prefix: S, err: Self) -> Self {
        Error::String(format!("{}: {:?}", prefix.as_ref(), err))
    }

    /// Wraps an error message with a prefix, creating a contextual error
    pub fn wrap_io<S: AsRef<str>>(prefix: S, err: std::io::Error) -> Error {
        Error::String(format!("{}: {:?}", prefix.as_ref(), err))
    }
}

/// Describes a failed yaml deserialization
///
/// This error contains the original yaml document
/// and aims to provide more verbose errors to help users
/// identify and resolve issues
#[derive(thiserror::Error, Debug)]
pub struct InvalidYamlError {
    pub yaml: String,
    pub err: serde_yaml::Error,
}

impl std::fmt::Display for InvalidYamlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.yaml.trim().is_empty() {
            // empty yaml is not valid
            // also check '---' ?
            return f.write_str("Yaml was completely empty");
        }
        let loc = match self.err.location() {
            Some(loc) => loc,
            None => {
                // just the error message, then?
                return f.write_fmt(format_args!("{}", self.err));
            }
        };

        f.write_fmt(format_args!("{}\n", self.err))?;
        let reader = std::io::BufReader::new(std::io::Cursor::new(&self.yaml));
        let lines = reader.lines().enumerate();
        let mut lines = match loc.line() {
            0 | 1 | 2 => lines.skip(0),
            pos => lines.skip(pos - 2),
        };
        if loc.line() > 1 {
            let (line_no, line) = lines.next().unwrap();
            f.write_fmt(format_args!("{line_no:03} | "))?;
            f.write_str(&line.unwrap())?;
        }
        if loc.line() > 2 {
            let (line_no, line) = lines.next().unwrap();
            f.write_fmt(format_args!("{line_no:03} | "))?;
            f.write_str(&line.unwrap())?;
        }
        let (line_no, target_line) = lines.next().expect("reported error location was wrong");
        f.write_fmt(format_args!("{line_no:03} | "))?;
        f.write_str(&target_line.unwrap())?;
        f.write_char('\n')?;
        f.write_fmt(format_args!("    |{:-width$}^\n", "", width = loc.column()))?;
        if let Some((line_no, Ok(line))) = lines.next() {
            f.write_fmt(format_args!("{line_no:03} | "))?;
            f.write_str(&line)?;
        }
        if let Some((line_no, Ok(line))) = lines.next() {
            f.write_fmt(format_args!("{line_no:03} | "))?;
            f.write_str(&line)?;
        }
        Ok(())
    }
}
