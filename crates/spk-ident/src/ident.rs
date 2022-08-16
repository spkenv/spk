// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::{convert::TryFrom, fmt::Write, str::FromStr};

use relative_path::RelativePathBuf;
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use spk_foundation::ident_ops::MetadataPath;

use crate::{parsing, RangeIdent, Result};
use spk_foundation::ident_build::Build;
use spk_foundation::version::Version;
use spk_name::{PkgNameBuf, RepositoryNameBuf};

#[cfg(test)]
#[path = "./ident_test.rs"]
mod ident_test;

/// Parse an identifier from a string.
///
/// This will panic if the identifier is wrong,
/// and should only be used for testing.
///
/// ```
/// # #[macro_use] extern crate spk_ident;
/// # fn main() {
/// ident!("my-pkg/1.0.0");
/// # }
/// ```
#[macro_export]
macro_rules! ident {
    ($ident:literal) => {
        $crate::parse_ident($ident).unwrap()
    };
}

/// Ident represents a package identifier.
///
/// The identifier is either a specific package or
/// range of package versions/releases depending on the
/// syntax and context
#[derive(Clone, Hash, PartialEq, Eq, Ord, PartialOrd)]
pub struct Ident {
    pub name: PkgNameBuf,
    pub version: Version,
    pub build: Option<Build>,
}

impl std::fmt::Debug for Ident {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Ident").field(&self.to_string()).finish()
    }
}

impl std::fmt::Display for Ident {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(self.name.as_str())?;
        if let Some(vb) = self.version_and_build() {
            f.write_char('/')?;
            f.write_str(vb.as_str())?;
        }
        Ok(())
    }
}

impl Ident {
    pub fn new(name: PkgNameBuf) -> Self {
        Self {
            name,
            version: Default::default(),
            build: Default::default(),
        }
    }

    /// Return true if this identifier is for a source package.
    pub fn is_source(&self) -> bool {
        match &self.build {
            Some(build) => build.is_source(),
            None => false,
        }
    }

    /// Return a copy of this identifier with the given version number instead
    pub fn with_version(&self, version: Version) -> Ident {
        Self {
            name: self.name.clone(),
            version,
            build: self.build.clone(),
        }
    }

    /// Set the build component of this package identifier.
    pub fn set_build(&mut self, build: Option<Build>) {
        self.build = build;
    }

    /// Return a copy of this identifier with the given build replaced.
    pub fn with_build(&self, build: Option<Build>) -> Self {
        let mut new = self.clone();
        new.build = build;
        new
    }

    /// Turn this identifier into one for the given build.
    pub fn into_build(mut self, build: Build) -> Self {
        // TODO: return a non-null build identifier type
        self.build = Some(build);
        self
    }

    /// Convert into a [`BuildIdent`] with the given [`RepositoryNameBuf`].
    ///
    /// A build must be assigned.
    pub fn try_into_build_ident(
        mut self,
        repository_name: RepositoryNameBuf,
    ) -> Result<BuildIdent> {
        self.build
            .take()
            .map(|build| BuildIdent {
                repository_name,
                name: self.name,
                version: self.version,
                build,
            })
            .ok_or_else(|| "Ident must contain a build to become a BuildIdent".into())
    }

    /// A string containing the properly formatted name and version number
    ///
    /// This is the same as [`ToString::to_string`] when the build is None.
    pub fn version_and_build(&self) -> Option<String> {
        match &self.build {
            Some(build) => Some(format!("{}/{}", self.version, build.digest())),
            None => {
                if self.version.is_zero() {
                    None
                } else {
                    Some(self.version.to_string())
                }
            }
        }
    }
}

impl MetadataPath for Ident {
    fn metadata_path(&self) -> RelativePathBuf {
        let path = RelativePathBuf::from(self.name.as_str());
        if let Some(vb) = self.version_and_build() {
            path.join(vb.as_str())
        } else {
            path
        }
    }
}

impl From<PkgNameBuf> for Ident {
    fn from(n: PkgNameBuf) -> Self {
        Self::new(n)
    }
}

impl TryFrom<RangeIdent> for Ident {
    type Error = crate::Error;

    fn try_from(ri: RangeIdent) -> Result<Self> {
        let name = ri.name;
        let build = ri.build;
        Ok(ri.version.try_into_version().map(|version| Self {
            name,
            version,
            build,
        })?)
    }
}

impl TryFrom<&RangeIdent> for Ident {
    type Error = crate::Error;

    fn try_from(ri: &RangeIdent) -> Result<Self> {
        Ok(ri.version.clone().try_into_version().map(|version| Self {
            name: ri.name.clone(),
            version,
            build: ri.build.clone(),
        })?)
    }
}

impl TryFrom<&str> for Ident {
    type Error = crate::Error;

    fn try_from(value: &str) -> Result<Self> {
        Self::from_str(value)
    }
}

impl TryFrom<&String> for Ident {
    type Error = crate::Error;

    fn try_from(value: &String) -> Result<Self> {
        Self::from_str(value.as_str())
    }
}

impl TryFrom<String> for Ident {
    type Error = crate::Error;

    fn try_from(value: String) -> Result<Self> {
        Self::from_str(value.as_str())
    }
}

impl FromStr for Ident {
    type Err = crate::Error;

    /// Parse the given identifier string into this instance.
    fn from_str(source: &str) -> Result<Self> {
        parsing::ident::<nom_supreme::error::ErrorTree<_>>(source)
            .map(|(_, ident)| ident)
            .map_err(|err| match err {
                nom::Err::Error(e) | nom::Err::Failure(e) => crate::Error::String(e.to_string()),
                nom::Err::Incomplete(_) => unreachable!(),
            })
    }
}

/// Parse a package identifier string.
pub fn parse_ident<S: AsRef<str>>(source: S) -> Result<Ident> {
    Ident::from_str(source.as_ref())
}

impl Serialize for Ident {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}
impl<'de> Deserialize<'de> for Ident {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::from_str(&s).map_err(de::Error::custom)
    }
}

/// BuildIdent represents a specific package build.
///
/// Like [`Ident`], except a [`RepositoryNameBuf`] and [`Build`] are required.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct BuildIdent {
    pub repository_name: RepositoryNameBuf,
    pub name: PkgNameBuf,
    pub version: Version,
    pub build: Build,
}

impl BuildIdent {
    /// Return true if this identifier is for a source package.
    pub fn is_source(&self) -> bool {
        self.build.is_source()
    }

    pub fn name(&self) -> &str {
        self.name.as_str()
    }
}

impl MetadataPath for BuildIdent {
    fn metadata_path(&self) -> RelativePathBuf {
        // The data path *does not* include the repository name.
        RelativePathBuf::from(self.name.as_str())
            .join(self.version.to_string())
            .join(self.build.to_string())
    }
}

impl std::fmt::Display for BuildIdent {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(self.repository_name.as_str())?;
        f.write_char('/')?;
        f.write_str(self.name.as_str())?;
        f.write_char('/')?;
        f.write_str(self.version.to_string().as_str())?;
        f.write_char('/')?;
        f.write_str(self.build.to_string().as_str())?;
        Ok(())
    }
}

impl From<BuildIdent> for Ident {
    fn from(bi: BuildIdent) -> Self {
        Ident {
            name: bi.name,
            version: bi.version,
            build: Some(bi.build),
        }
    }
}

impl From<&BuildIdent> for Ident {
    fn from(bi: &BuildIdent) -> Self {
        Ident {
            name: bi.name.clone(),
            version: bi.version.clone(),
            build: Some(bi.build.clone()),
        }
    }
}
