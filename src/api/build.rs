// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::convert::TryInto;
use std::str::FromStr;

use itertools::Itertools;

#[cfg(test)]
#[path = "./build_test.rs"]
mod build_test;

const SRC: &str = "src";
const EMBEDDED: &str = "embedded";

/// Denotes that an invalid build digest was given.
#[derive(Debug)]
pub struct InvalidBuildError {
    pub message: String,
}

impl InvalidBuildError {
    pub fn new(msg: String) -> crate::Error {
        crate::Error::InvalidBuildError(Self { message: msg })
    }
}

/// Build represents a package build identifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Build {
    Source,
    Embedded,
    Digest([char; super::option_map::DIGEST_SIZE]),
}

impl Build {
    /// The name or digest of this build as shown in a version
    pub fn digest(&self) -> String {
        match self {
            Build::Source => SRC.to_string(),
            Build::Embedded => EMBEDDED.to_string(),
            Build::Digest(d) => d.iter().collect(),
        }
    }

    pub fn is_source(&self) -> bool {
        if let Build::Source = self {
            true
        } else {
            false
        }
    }

    pub fn is_emdeded(&self) -> bool {
        if let Build::Embedded = self {
            true
        } else {
            false
        }
    }
}

impl std::fmt::Display for Build {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(self.digest().as_str())
    }
}

impl FromStr for Build {
    type Err = crate::Error;

    fn from_str(source: &str) -> crate::Result<Self> {
        match source {
            SRC => Ok(Build::Source),
            EMBEDDED => Ok(Build::Embedded),
            _ => {
                if let Err(err) = data_encoding::BASE32.decode(source.as_bytes()) {
                    return Err(InvalidBuildError::new(format!(
                        "Invalid build digest '{}': {:?}",
                        source, err
                    )));
                }

                match source.chars().collect_vec().try_into() {
                    Ok(chars) => Ok(Build::Digest(chars)),

                    Err(err) => Err(InvalidBuildError::new(format!(
                        "Invalid build digest '{}': {:?}",
                        source, err
                    ))),
                }
            }
        }
    }
}

/// Parse the given string as a build identifier
pub fn parse_build<S: AsRef<str>>(digest: S) -> crate::Result<Build> {
    Build::from_str(digest.as_ref())
}
