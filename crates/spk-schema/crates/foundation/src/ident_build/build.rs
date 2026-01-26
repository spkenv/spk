// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::collections::BTreeSet;
use std::str::FromStr;

use derive_where::derive_where;
use miette::Diagnostic;
use relative_path::RelativePathBuf;
use thiserror::Error;

use super::BuildId;
use crate::ident::{self, PkgRequest, RangeIdent, RequestedBy};
use crate::ident_component::{Component, Components};
use crate::ident_ops::parsing::IdentPartsBuf;
use crate::ident_ops::{MetadataPath, TagPath};
use crate::version::Version;
use crate::version_range::{DoubleEqualsVersion, VersionFilter, VersionRange};

#[cfg(test)]
#[path = "./build_test.rs"]
mod build_test;

pub const SRC: &str = "src";
pub const EMBEDDED: &str = "embedded";

/// Denotes that an invalid build digest was given.
#[derive(Diagnostic, Debug, Error)]
#[error("Invalid build: {message}")]
pub struct InvalidBuildError {
    pub message: String,
}

impl InvalidBuildError {
    pub fn new_error(msg: String) -> super::Error {
        super::Error::InvalidBuildError(Self { message: msg })
    }
}

#[derive(Clone, Debug)]
#[derive_where(Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EmbeddedSourcePackage {
    pub ident: IdentPartsBuf,
    pub components: BTreeSet<Component>,
    /// The original unparsed tag, if known, which may contain non-normalized
    /// version numbers due to legacy builds of spk allowing this.
    ///
    /// It is not considered for comparisons or hashing but will be used when
    /// generating the metadata path to preserve the ability to roundtrip back
    /// to the existing tag on disk.
    #[derive_where(skip(EqHashOrd))]
    pub unparsed: Option<String>,
}

impl EmbeddedSourcePackage {
    pub const EMBEDDED_BY_PREFIX: &'static str = "embedded-by-";

    /// Create a [`PkgRequest'] representing the package that embeds this one.
    pub fn to_pkg_request(&self, requester: RequestedBy) -> ident::Result<PkgRequest> {
        let Some(version_str) = &self.ident.version_str else {
            return Err(ident::Error::String(
                "embedded source package must have a version".to_string(),
            ));
        };
        let Some(build_str) = &self.ident.build_str else {
            return Err(ident::Error::String(
                "embedded source package must have a build".to_string(),
            ));
        };

        let ri = RangeIdent {
            repository_name: None,
            name: self.ident.pkg_name.parse()?,
            version: VersionFilter::single(VersionRange::DoubleEquals(DoubleEqualsVersion::from(
                version_str.as_str().parse::<Version>()?,
            ))),
            components: self.components.clone(),
            build: Some(build_str.parse()?),
        };
        Ok(PkgRequest::new(ri, requester))
    }
}

/// An embedded package's source (if known).
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum EmbeddedSource {
    // Boxed to keep enum size down for enums that use this enum.
    Package(Box<EmbeddedSourcePackage>),
    Unknown,
}

impl MetadataPath for EmbeddedSource {
    fn metadata_path(&self) -> RelativePathBuf {
        match self {
            package @ EmbeddedSource::Package(esp) => RelativePathBuf::from(format!(
                "{}{}",
                EmbeddedSourcePackage::EMBEDDED_BY_PREFIX,
                // Encode the parent ident into base32 to have a unique value
                // per unique parent that is a valid filename. The trailing
                // '=' are not allowed in tag names (use NOPAD).
                data_encoding::BASE32_NOPAD.encode(
                    esp.unparsed
                        .as_ref()
                        .map(|unparsed| format!("{EMBEDDED}{unparsed}"))
                        .unwrap_or_else(|| package.to_string())
                        .as_bytes()
                )
            )),
            EmbeddedSource::Unknown => RelativePathBuf::from("embedded"),
        }
    }
}

impl std::fmt::Display for EmbeddedSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{EMBEDDED}")?;
        match self {
            EmbeddedSource::Package(package) => {
                // XXX this is almost the same code as `RangeIdent::fmt()`.
                write!(f, "[")?;
                package.ident.pkg_name.fmt(f)?;
                package.components.fmt_component_set(f)?;
                if let Some(version) = &package.ident.version_str {
                    write!(f, "/")?;
                    version.fmt(f)?;
                }
                if let Some(build) = &package.ident.build_str {
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
    BuildId(BuildId),
}

impl Build {
    /// An empty build is the build digest created from
    /// an empty option map
    pub fn empty() -> &'static Build {
        static EMPTY: Build =
            Build::BuildId(BuildId::new(['3', 'I', '4', '2', 'H', '3', 'S', '6']));
        &EMPTY
    }

    /// A null build is the build digest created by
    /// encoded a series of all zeros (ie: [`spfs::encoding::NULL_DIGEST`])
    pub fn null() -> &'static Build {
        static NULL: Build = Build::BuildId(BuildId::new(['A', 'A', 'A', 'A', 'A', 'A', 'A', 'A']));
        &NULL
    }

    /// True if this build is equal to the null one, see [`Build::null`]
    pub fn is_null(&self) -> bool {
        self == Self::null()
    }

    /// The name or digest of this build as shown in a version
    pub fn digest(&self) -> String {
        match self {
            Build::Source => SRC.to_string(),
            Build::Embedded(by) => by.to_string(),
            Build::BuildId(d) => d.to_string(),
        }
    }

    pub fn is_buildid(&self) -> bool {
        matches!(self, Build::BuildId(_))
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
            Build::Source | Build::BuildId(_) => RelativePathBuf::from(self.digest()),
            Build::Embedded(embedded) => embedded.metadata_path(),
        }
    }
}

impl TagPath for Build {
    fn tag_path(&self) -> RelativePathBuf {
        self.metadata_path()
    }

    fn verbatim_tag_path(&self) -> RelativePathBuf {
        // No difference between verbatim and non-verbatim for builds, which
        // don't hold a version number (except for embedded, where it doesn't
        // matter(?)).
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

impl TryFrom<String> for Build {
    type Error = super::Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::from_str(&value)
    }
}

impl FromStr for Build {
    type Err = super::Error;

    fn from_str(source: &str) -> super::Result<Self> {
        use nom::combinator::all_consuming;

        all_consuming(super::parsing::build::<nom_supreme::error::ErrorTree<_>>)(source)
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
pub fn parse_build<S: AsRef<str>>(digest: S) -> super::Result<Build> {
    Build::from_str(digest.as_ref())
}
