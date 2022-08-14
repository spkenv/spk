// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::{convert::TryFrom, fmt::Write, str::FromStr};

use relative_path::RelativePathBuf;
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

use crate::{parsing, Result};

use super::{
    recipe::VersionedMut, Build, Builded, Named, PkgNameBuf, RangeIdent, RepositoryNameBuf,
    Version, Versioned,
};

#[cfg(test)]
#[path = "./ident_test.rs"]
mod ident_test;

/// Parse an identifier from a string.
///
/// This will panic if the identifier is wrong,
/// and should only be used for testing.
///
/// ```
/// # #[macro_use] extern crate spk;
/// # fn main() {
/// ident!("my-pkg/1.0.0");
/// # }
/// ```
#[macro_export]
macro_rules! ident {
    ($ident:literal) => {
        $crate::api::parse_ident($ident).unwrap()
    };
}

pub trait MetadataPath {
    /// Return the relative path for package metadata for an ident.
    ///
    /// Package metadata is stored on disk within each package, for example:
    ///     /spfs/spk/pkg/pkg-name/1.0.0/CU7ZWOIF
    ///
    /// This method should return only the ident part:
    ///     pkg-name/1.0.0/CU7ZWOIF
    fn metadata_path(&self) -> RelativePathBuf;
}

/// Ident represents a package identifier.
///
/// The identifier is either a specific package or
/// range of package versions/releases depending on the
/// syntax and context
pub struct Ident<Id = BuildId>(Id)
where
    Id: Named + Versioned;

impl<Id> Ident<Id>
where
    Id: Named + Versioned,
{
    pub fn new(id: Id) -> Self {
        Self(id)
    }

    /// Turn this identifier into one for the given build.
    pub fn into_build(self, build: Build) -> Ident {
        // TODO: use a trait to allow breaking down and not cloning data
        // TODO: return a non-null build identifier type
        Ident(BuildId {
            name: self.name().to_owned(),
            version: self.version().clone(),
            build: Some(build),
        })
    }

    /// Deconstruct this ident, returning the inner identifier
    pub fn into_inner(self) -> Id {
        self.0
    }
}

impl<Id> Named for Ident<Id>
where
    Id: Named + Versioned,
{
    fn name(&self) -> &super::PkgName {
        self.0.name()
    }
}

impl<Id> Versioned for Ident<Id>
where
    Id: Named + Versioned,
{
    fn version(&self) -> &super::Version {
        self.0.version()
    }
}

impl<Id> VersionedMut for Ident<Id>
where
    Id: Named + Versioned + VersionedMut,
{
    fn set_version(&mut self, version: super::Version) {
        self.0.set_version(version)
    }
}

impl<Id> Builded for Ident<Id>
where
    Id: Named + Versioned + Builded,
{
    fn build(&self) -> Option<&Build> {
        self.0.build()
    }

    fn set_build(&mut self, build: Option<Build>) {
        self.0.set_build(build)
    }
}

impl<Id> std::ops::Deref for Ident<Id>
where
    Id: Named + Versioned,
{
    type Target = Id;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<Id> std::fmt::Display for Ident<Id>
where
    Id: Named + Versioned + std::fmt::Display,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl<Id> std::fmt::Debug for Ident<Id>
where
    Id: Named + Versioned + ToString,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("BuildIdent")
            .field(&self.to_string())
            .finish()
    }
}

impl<Id> MetadataPath for Ident<Id>
where
    Id: Named + Versioned + MetadataPath,
{
    fn metadata_path(&self) -> RelativePathBuf {
        self.0.metadata_path()
    }
}

impl TryFrom<RangeIdent> for Ident {
    type Error = crate::Error;

    fn try_from(ri: RangeIdent) -> Result<Self> {
        let name = ri.name;
        let build = ri.build;
        ri.version.try_into_version().map(|version| {
            Self(BuildId {
                name,
                version,
                build,
            })
        })
    }
}

impl TryFrom<&RangeIdent> for Ident {
    type Error = crate::Error;

    fn try_from(ri: &RangeIdent) -> Result<Self> {
        ri.version.clone().try_into_version().map(|version| {
            Self(BuildId {
                name: ri.name.clone(),
                version,
                build: ri.build.clone(),
            })
        })
    }
}

impl<Id> FromStr for Ident<Id>
where
    Id: Named + Versioned + FromStr,
{
    type Err = <Id as FromStr>::Err;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Id::from_str(s).map(Self)
    }
}

impl<Id> From<PkgNameBuf> for Ident<Id>
where
    Id: Named + Versioned + From<PkgNameBuf>,
{
    fn from(source: PkgNameBuf) -> Self {
        Self(source.into())
    }
}

impl<Id> Serialize for Ident<Id>
where
    Id: Named + Versioned + ToString,
{
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0.to_string())
    }
}

impl<'de, Id> Deserialize<'de> for Ident<Id>
where
    Id: Named + Versioned + FromStr,
    <Id as FromStr>::Err: std::fmt::Display,
{
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Id::from_str(&s).map_err(de::Error::custom).map(Self)
    }
}

impl<Id> std::clone::Clone for Ident<Id>
where
    Id: Named + Versioned + std::clone::Clone,
{
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<Id> std::hash::Hash for Ident<Id>
where
    Id: Named + Versioned + std::hash::Hash,
{
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

impl<Id> std::cmp::PartialEq for Ident<Id>
where
    Id: Named + Versioned + std::cmp::PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl<Id> std::cmp::Eq for Ident<Id> where Id: Named + Versioned + std::cmp::Eq {}

impl<Id> std::cmp::Ord for Ident<Id>
where
    Id: Named + Versioned + std::cmp::Ord,
{
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.cmp(&other.0)
    }
}

impl<Id> std::cmp::PartialOrd for Ident<Id>
where
    Id: Named + Versioned + std::cmp::PartialOrd,
{
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.0.partial_cmp(&other.0)
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, Ord, PartialOrd)]
pub struct BuildId {
    pub name: PkgNameBuf,
    pub version: Version,
    pub build: Option<Build>,
}

impl Named for BuildId {
    fn name(&self) -> &super::PkgName {
        &self.name
    }
}

impl Versioned for BuildId {
    fn version(&self) -> &super::Version {
        &self.version
    }
}

impl VersionedMut for BuildId {
    fn set_version(&mut self, version: super::Version) {
        self.version = version
    }
}

impl Builded for BuildId {
    fn build(&self) -> Option<&Build> {
        self.build.as_ref()
    }

    fn set_build(&mut self, build: Option<Build>) {
        self.build = build
    }
}

impl std::fmt::Display for BuildId {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(self.name.as_str())?;
        if let Some(vb) = self.version_and_build() {
            f.write_char('/')?;
            f.write_str(vb.as_str())?;
        }
        Ok(())
    }
}

impl BuildId {
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
        Ident::new(Self {
            name: self.name.clone(),
            version,
            build: self.build.clone(),
        })
    }

    /// Return a copy of this identifier with the given build replaced.
    pub fn with_build(&self, build: Option<Build>) -> Ident {
        let mut new = self.clone();
        new.build = build;
        Ident::new(new)
    }

    /// Convert into a [`BuildId`] with the given [`RepositoryNameBuf`].
    ///
    /// A build must be assigned.
    pub fn try_into_build_ident(
        mut self,
        repository_name: RepositoryNameBuf,
    ) -> Result<PlacedBuildId> {
        self.build
            .take()
            .map(|build| PlacedBuildId {
                repository_name,
                name: self.name,
                version: self.version,
                build,
            })
            .ok_or_else(|| "BuildIdent must contain a build to become a BuildIdent".into())
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

impl MetadataPath for BuildId {
    fn metadata_path(&self) -> RelativePathBuf {
        let path = RelativePathBuf::from(self.name.as_str());
        if let Some(vb) = self.version_and_build() {
            path.join(vb.as_str())
        } else {
            path
        }
    }
}

impl From<PkgNameBuf> for BuildId {
    fn from(n: PkgNameBuf) -> Self {
        Self::new(n)
    }
}

impl TryFrom<&str> for BuildId {
    type Error = crate::Error;

    fn try_from(value: &str) -> Result<Self> {
        Self::from_str(value)
    }
}

impl TryFrom<&String> for BuildId {
    type Error = crate::Error;

    fn try_from(value: &String) -> Result<Self> {
        Self::from_str(value.as_str())
    }
}

impl TryFrom<String> for BuildId {
    type Error = crate::Error;

    fn try_from(value: String) -> Result<Self> {
        Self::from_str(value.as_str())
    }
}

impl FromStr for BuildId {
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
    BuildId::from_str(source.as_ref()).map(Ident::new)
}

/// BuildIdent represents a specific package build.
///
/// Like [`BuildId`], except a [`RepositoryNameBuf`] and [`Build`] are required.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct PlacedBuildId {
    pub repository_name: RepositoryNameBuf,
    pub name: PkgNameBuf,
    pub version: Version,
    pub build: Build,
}

impl PlacedBuildId {
    /// Return true if this identifier is for a source package.
    pub fn is_source(&self) -> bool {
        self.build.is_source()
    }

    pub fn name(&self) -> &str {
        self.name.as_str()
    }
}

impl MetadataPath for PlacedBuildId {
    fn metadata_path(&self) -> RelativePathBuf {
        // The data path *does not* include the repository name.
        RelativePathBuf::from(self.name.as_str())
            .join(self.version.to_string())
            .join(self.build.to_string())
    }
}

impl std::fmt::Display for PlacedBuildId {
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

impl From<PlacedBuildId> for BuildId {
    fn from(bi: PlacedBuildId) -> Self {
        BuildId {
            name: bi.name,
            version: bi.version,
            build: Some(bi.build),
        }
    }
}

impl From<&PlacedBuildId> for BuildId {
    fn from(bi: &PlacedBuildId) -> Self {
        BuildId {
            name: bi.name.clone(),
            version: bi.version.clone(),
            build: Some(bi.build.clone()),
        }
    }
}
