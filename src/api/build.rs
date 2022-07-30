// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::{collections::BTreeSet, str::FromStr};

use relative_path::RelativePathBuf;
use thiserror::Error;

use super::{
    component_spec::Components,
    ident::{MetadataPath, TagPath},
    Ident,
};

#[cfg(test)]
#[path = "./build_test.rs"]
mod build_test;

pub(crate) const SRC: &str = "src";
pub(crate) const EMBEDDED: &str = "embedded";

/// Denotes that an invalid build digest was given.
#[derive(Debug, Error)]
#[error("Invalid build: {message}")]
pub struct InvalidBuildError {
    pub message: String,
}

impl InvalidBuildError {
    pub fn new_error(msg: String) -> crate::Error {
        crate::Error::InvalidBuildError(Self { message: msg })
    }
}

/// An embedded package's source (if known).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum EmbeddedSource {
    Package {
        ident: Box<Ident>,
        components: BTreeSet<super::Component>,
    },
    Unknown,
}

impl MetadataPath for EmbeddedSource {
    fn metadata_path(&self) -> RelativePathBuf {
        match self {
            package @ EmbeddedSource::Package { .. } => RelativePathBuf::from(format!(
                "embedded-by-{}",
                // Encode the parent ident into base32 to have a unique value
                // per unique parent that is a valid filename. The trailing
                // '=' are not allowed in tag names (use NOPAD).
                data_encoding::BASE32_NOPAD.encode(package.to_string().as_bytes())
            )),
            EmbeddedSource::Unknown => RelativePathBuf::from("embedded"),
        }
    }
}

impl std::fmt::Display for EmbeddedSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{EMBEDDED}")?;
        match self {
            EmbeddedSource::Package { ident, components } => {
                // XXX this is almost the same code as `RangeIdent::fmt()`.
                write!(f, "[")?;
                ident.name.fmt(f)?;
                components.fmt_component_set(f)?;
                write!(f, "/")?;
                ident.version.fmt(f)?;
                if let Some(build) = &ident.build {
                    write!(f, "/")?;
                    build.fmt(f)?;
                }
                write!(f, "]")
            }
            EmbeddedSource::Unknown => Ok(()),
        }
    }
}

/// Build represents a package build identifier.
#[derive(Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum Build {
    Source,
    Embedded(EmbeddedSource),
    Digest([char; super::option_map::DIGEST_SIZE]),
}

impl Build {
    /// The name or digest of this build as shown in a version
    pub fn digest(&self) -> String {
        match self {
            Build::Source => SRC.to_string(),
            Build::Embedded(by) => by.to_string(),
            Build::Digest(d) => d.iter().collect(),
        }
    }

    pub fn is_source(&self) -> bool {
        matches!(self, Build::Source)
    }

    pub fn is_embedded(&self) -> bool {
        matches!(self, Build::Embedded(_))
    }

    pub fn is_embed_stub(&self) -> bool {
        matches!(self, Build::Embedded(EmbeddedSource::Package { .. }))
    }
}

impl MetadataPath for Build {
    fn metadata_path(&self) -> RelativePathBuf {
        match self {
            Build::Source | Build::Digest(_) => RelativePathBuf::from(self.digest()),
            Build::Embedded(embedded) => embedded.metadata_path(),
        }
    }
}

impl TagPath for Build {
    fn tag_path(&self) -> RelativePathBuf {
        self.metadata_path()
    }
}

impl std::fmt::Debug for Build {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(self.digest().as_str())
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
        use nom::combinator::all_consuming;

        all_consuming(crate::parsing::build::<nom_supreme::error::ErrorTree<_>>)(source)
            .map(|(_, build)| build)
            .map_err(|err| match err {
                nom::Err::Error(e) | nom::Err::Failure(e) => {
                    InvalidBuildError::new_error(e.to_string())
                }
                nom::Err::Incomplete(_) => unreachable!(),
            })
    }
}

/// Parse the given string as a build identifier
pub fn parse_build<S: AsRef<str>>(digest: S) -> crate::Result<Build> {
    Build::from_str(digest.as_ref())
}
