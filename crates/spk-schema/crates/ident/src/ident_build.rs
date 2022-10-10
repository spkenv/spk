// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::fmt::Write;
use std::str::FromStr;

use relative_path::RelativePathBuf;
use spk_schema_foundation::ident_build::Build;
use spk_schema_foundation::ident_ops::parsing::IdentPartsBuf;
use spk_schema_foundation::ident_ops::{MetadataPath, TagPath};
use spk_schema_foundation::name::{PkgName, PkgNameBuf, RepositoryNameBuf};
use spk_schema_foundation::spec_ops::prelude::*;
use spk_schema_foundation::version::Version;

use crate::ident_version::VersionIdent;
use crate::{parsing, AnyIdent, Error, Ident, LocatedBuildIdent, RangeIdent, Result};

/// Identifies a specific package version and build.
pub type BuildIdent = Ident<VersionIdent, Build>;

impl BuildIdent {
    /// The name of the identified package.
    pub fn name(&self) -> &PkgName {
        self.base().name()
    }

    /// The version number identified for this package
    pub fn version(&self) -> &Version {
        self.base().version()
    }

    // The build id identified for this package
    pub fn build(&self) -> &Build {
        self.target()
    }

    /// Return if this identifier can possibly have embedded packages.
    pub fn can_embed(&self) -> bool {
        // Only builds can have embeds.
        matches!(self.build(), Build::Digest(_))
    }

    /// Return true if this identifier is for an embedded package.
    pub fn is_embedded(&self) -> bool {
        matches!(self.build(), Build::Embedded(_))
    }

    /// Return true if this identifier is for a source package.
    pub fn is_source(&self) -> bool {
        self.build().is_source()
    }

    /// Return a copy of this identifier with the given version number instead
    pub fn with_version(&self, version: Version) -> Self {
        Self::new(self.base().with_version(version), self.target().clone())
    }

    /// Set the build component of this package identifier.
    pub fn set_build(&mut self, build: Build) {
        self.target = build;
    }

    /// Return a copy of this identifier with the given build replaced.
    pub fn with_build(&self, build: Build) -> Self {
        let mut new = self.clone();
        new.set_build(build);
        new
    }

    /// Turn this identifier into an [`AnyIdent`]
    pub fn into_any(self) -> AnyIdent {
        AnyIdent::new(self.base, Some(self.target))
    }

    /// Turn a copy of this identifier into an [`AnyIdent`]
    pub fn to_any(&self) -> AnyIdent {
        self.clone().into_any()
    }

    /// Convert into a [`LocatedBuildIdent`] with the given [`RepositoryNameBuf`].
    pub fn into_located(self, repository_name: RepositoryNameBuf) -> LocatedBuildIdent {
        LocatedBuildIdent {
            base: repository_name,
            target: self,
        }
    }
}

impl Named for BuildIdent {
    fn name(&self) -> &PkgName {
        self.name()
    }
}

impl HasVersion for BuildIdent {
    fn version(&self) -> &Version {
        self.base.version()
    }
}

impl HasBuild for BuildIdent {
    fn build(&self) -> &Build {
        &self.target
    }
}

impl std::fmt::Display for BuildIdent {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.base.fmt(f)?;
        f.write_char('/')?;
        self.target.fmt(f)
    }
}

impl FromStr for BuildIdent {
    type Err = crate::Error;

    /// Parse the given identifier string into this instance.
    fn from_str(source: &str) -> Result<Self> {
        use nom::combinator::all_consuming;

        all_consuming(parsing::build_ident::<nom_supreme::error::ErrorTree<_>>)(source)
            .map(|(_, ident)| ident)
            .map_err(|err| match err {
                nom::Err::Error(e) | nom::Err::Failure(e) => crate::Error::String(e.to_string()),
                nom::Err::Incomplete(_) => unreachable!(),
            })
    }
}

impl MetadataPath for BuildIdent {
    fn metadata_path(&self) -> RelativePathBuf {
        RelativePathBuf::from(self.name().as_str())
            .join(self.version().metadata_path())
            .join(self.build().metadata_path())
    }
}

impl TryFrom<&IdentPartsBuf> for BuildIdent {
    type Error = crate::Error;

    fn try_from(parts: &IdentPartsBuf) -> Result<Self> {
        if parts.repository_name.is_some() {
            return Err("Ident may not have a repository name".into());
        }

        let name = parts.pkg_name.parse::<PkgNameBuf>()?;
        let version = parts
            .version_str
            .as_ref()
            .map(|v| v.parse::<Version>())
            .transpose()?
            .unwrap_or_default();
        let build = parts
            .build_str
            .as_ref()
            .map(|v| v.parse::<Build>())
            .transpose()?
            .ok_or_else(|| Error::String("Ident must have an associated build id".into()))?;

        Ok(VersionIdent::new(name, version).into_build(build))
    }
}

impl TagPath for BuildIdent {
    fn tag_path(&self) -> RelativePathBuf {
        RelativePathBuf::from(self.name().as_str())
            .join(self.version().tag_path())
            .join(self.build().tag_path())
    }
}

impl From<&BuildIdent> for IdentPartsBuf {
    fn from(ident: &BuildIdent) -> Self {
        IdentPartsBuf {
            repository_name: None,
            pkg_name: ident.name().to_string(),
            version_str: Some(ident.version().to_string()),
            build_str: Some(ident.build().to_string()),
        }
    }
}

impl PartialEq<&BuildIdent> for IdentPartsBuf {
    fn eq(&self, other: &&BuildIdent) -> bool {
        self.repository_name.is_none()
            && self.pkg_name == other.name().as_str()
            && self.version_str == Some(other.version().to_string())
            && self.build_str == Some(other.build().to_string())
    }
}

impl TryFrom<RangeIdent> for BuildIdent {
    type Error = crate::Error;

    fn try_from(ri: RangeIdent) -> Result<Self> {
        let name = ri.name;
        let build = ri
            .build
            .ok_or_else(|| Error::String("Build was required from range ident".into()))?;
        Ok(ri
            .version
            .try_into_version()
            .map(|version| VersionIdent::new(name, version).into_build(build))?)
    }
}

impl TryFrom<&RangeIdent> for BuildIdent {
    type Error = crate::Error;

    fn try_from(ri: &RangeIdent) -> Result<Self> {
        ri.clone().try_into()
    }
}

/// Parse a package identifier string with associated build.
pub fn parse_build_ident<S: AsRef<str>>(source: S) -> Result<BuildIdent> {
    Ident::from_str(source.as_ref())
}
