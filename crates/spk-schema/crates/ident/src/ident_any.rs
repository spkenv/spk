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
use spk_schema_foundation::version::Version;

use crate::ident_version::VersionIdent;
use crate::{parsing, BuildIdent, Ident, LocatedBuildIdent, RangeIdent, Result};

#[cfg(test)]
#[path = "./ident_any_test.rs"]
mod ident_any_test;

/// Identifies a specific package version and build.
pub type AnyIdent = Ident<VersionIdent, Option<Build>>;

super::ident_version::version_ident_methods!(AnyIdent, .base);

impl AnyIdent {
    /// The build identified for this package, if any
    pub fn build(&self) -> Option<&Build> {
        self.target().as_ref()
    }

    /// Return a copy of this identifier with the given build instead
    pub fn with_build(&self, build: Option<Build>) -> Self {
        Self::new(self.base().clone(), build)
    }

    /// Reinterpret this identifier as a [`crate::VersionIdent`]
    pub fn as_version(&self) -> &VersionIdent {
        self.base()
    }

    /// Convert a copy of this identifier into a [`crate::VersionIdent`]
    pub fn to_version(self) -> VersionIdent {
        self.base().clone()
    }

    /// Convert this identifier into a [`crate::VersionIdent`]
    pub fn into_version(self) -> VersionIdent {
        self.into_base()
    }

    /// Return a copy of this pointing to the given build.
    pub fn to_build(&self, build: Build) -> BuildIdent {
        BuildIdent::new(self.base().clone(), build)
    }

    /// Convert this ident into a [`BuildIdent`] if possible
    pub fn into_build(self) -> Option<BuildIdent> {
        let (base, target) = self.into_inner();
        target.map(|build| BuildIdent::new(base, build))
    }
}

impl AnyIdent {
    /// Return if this identifier can possibly have embedded packages.
    pub fn can_embed(&self) -> bool {
        // Only builds can have embeds.
        matches!(self.build(), Some(Build::Digest(_)))
    }

    /// Return true if this identifier is for an embedded package.
    pub fn is_embedded(&self) -> bool {
        matches!(self.build(), Some(Build::Embedded(_)))
    }

    /// Return true if this identifier is for a source package.
    pub fn is_source(&self) -> bool {
        self.build().map(Build::is_source).unwrap_or_default()
    }

    /// A string containing the properly formatted name and version number
    ///
    /// This is the same as [`ToString::to_string`] when the build is None.
    pub fn version_and_build(&self) -> Option<String> {
        match self.build() {
            Some(build) => Some(format!("{}/{}", self.version(), build.digest())),
            None => {
                if self.version().is_zero() {
                    None
                } else {
                    Some(self.version().to_string())
                }
            }
        }
    }

    /// Convert into a [`LocatedBuildIdent`] with the given [`RepositoryNameBuf`].
    ///
    /// A build must be assigned.
    pub fn try_into_located_build_ident(
        self,
        repository_name: RepositoryNameBuf,
    ) -> Result<LocatedBuildIdent> {
        let (version_ident, build) = self.into_inner();
        build
            .map(|build| {
                LocatedBuildIdent::new(repository_name, BuildIdent::new(version_ident, build))
            })
            .ok_or_else(|| "Ident must contain a build to become a LocatedBuildIdent".into())
    }
}

impl std::fmt::Display for AnyIdent {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.base.name().fmt(f)?;
        if let Some(vb) = self.version_and_build() {
            f.write_char('/')?;
            f.write_str(&vb)?;
        }
        Ok(())
    }
}

impl MetadataPath for AnyIdent {
    fn metadata_path(&self) -> RelativePathBuf {
        let path = RelativePathBuf::from(self.name().as_str());
        match self.build() {
            Some(build) => path
                .join(self.version().metadata_path())
                .join(build.metadata_path()),
            None => {
                if self.version().is_zero() {
                    path
                } else {
                    path.join(self.version().metadata_path())
                }
            }
        }
    }
}

impl TagPath for AnyIdent {
    fn tag_path(&self) -> RelativePathBuf {
        let path = RelativePathBuf::from(self.name().as_str());
        match self.build() {
            Some(build) => path.join(self.version().tag_path()).join(build.tag_path()),
            None => {
                if self.version().is_zero() {
                    path
                } else {
                    path.join(self.version().tag_path())
                }
            }
        }
    }
}

impl FromStr for AnyIdent {
    type Err = crate::Error;

    /// Parse the given identifier string into this instance.
    fn from_str(source: &str) -> Result<Self> {
        use nom::combinator::all_consuming;

        all_consuming(parsing::ident::<nom_supreme::error::ErrorTree<_>>)(source)
            .map(|(_, ident)| ident)
            .map_err(|err| match err {
                nom::Err::Error(e) | nom::Err::Failure(e) => crate::Error::String(e.to_string()),
                nom::Err::Incomplete(_) => unreachable!(),
            })
    }
}

impl TryFrom<&IdentPartsBuf> for AnyIdent {
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
            .transpose()?;

        Ok(VersionIdent::new(name, version).into_any(build))
    }
}

impl From<&AnyIdent> for IdentPartsBuf {
    fn from(ident: &AnyIdent) -> Self {
        IdentPartsBuf {
            repository_name: None,
            pkg_name: ident.name().to_string(),
            version_str: Some(ident.version().to_string()),
            build_str: ident.build().map(|b| b.to_string()),
        }
    }
}

impl From<PkgNameBuf> for AnyIdent {
    fn from(name: PkgNameBuf) -> Self {
        VersionIdent::new_zero(name).into_any(None)
    }
}

impl PartialEq<&AnyIdent> for IdentPartsBuf {
    fn eq(&self, other: &&AnyIdent) -> bool {
        self.repository_name.is_none()
            && self.pkg_name == other.name().as_str()
            && self.version_str == Some(other.version().to_string())
            && self.build_str == other.build().map(|b| b.to_string())
    }
}

impl TryFrom<RangeIdent> for AnyIdent {
    type Error = crate::Error;

    fn try_from(ri: RangeIdent) -> Result<Self> {
        let name = ri.name;
        let build = ri.build;
        Ok(ri
            .version
            .try_into_version()
            .map(|version| VersionIdent::new(name, version).into_any(build))?)
    }
}

impl TryFrom<&RangeIdent> for AnyIdent {
    type Error = crate::Error;

    fn try_from(ri: &RangeIdent) -> Result<Self> {
        Ok(ri.version.clone().try_into_version().map(|version| {
            VersionIdent::new(ri.name.clone(), version).into_any(ri.build.clone())
        })?)
    }
}

/// Parse a package identifier string with optional build.
pub fn parse_ident<S: AsRef<str>>(source: S) -> Result<AnyIdent> {
    Ident::from_str(source.as_ref())
}
