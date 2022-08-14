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

#[enum_dispatch::enum_dispatch]
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
pub struct Ident<Id = AnyId>(Id)
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
        Ident(AnyId::Build(BuildId {
            name: self.name().to_owned(),
            version: self.version().clone(),
            build,
        }))
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
    fn build(&self) -> &Build {
        self.0.build()
    }

    fn set_build(&mut self, build: Build) {
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

impl<Id> std::ops::DerefMut for Ident<Id>
where
    Id: Named + Versioned,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
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
        ri.version.try_into_version().map(|version| match build {
            Some(build) => Self(AnyId::Build(BuildId {
                name,
                version,
                build,
            })),
            None => Self(AnyId::Version(VersionId { name, version })),
        })
    }
}

impl TryFrom<&RangeIdent> for Ident {
    type Error = crate::Error;

    fn try_from(ri: &RangeIdent) -> Result<Self> {
        ri.version
            .clone()
            .try_into_version()
            .map(|version| match &ri.build {
                Some(build) => Self(AnyId::Build(BuildId {
                    name: ri.name.clone(),
                    version,
                    build: build.clone(),
                })),
                None => Self(AnyId::Version(VersionId {
                    name: ri.name.clone(),
                    version,
                })),
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

/// Identifies a package with variable amounts of specificity.
#[derive(Clone, Debug, Hash, PartialEq, Eq, Ord, PartialOrd)]
#[enum_dispatch::enum_dispatch(Named, Versioned, VersionedMut, MetadataPath)]
pub enum AnyId {
    Version(VersionId),
    Build(BuildId),
    // TODO:
    // if there value in having this be an option here? or are there too
    // many places that would just end up needing to reject this variant?
    // PlacedBuild(PlacedBuildId),
}

impl AnyId {
    pub fn new(name: PkgNameBuf) -> Self {
        Self::Version(VersionId::new(name))
    }

    /// Return true if this identifier is for a source package.
    pub fn is_source(&self) -> bool {
        match self.build() {
            Some(build) => build.is_source(),
            None => false,
        }
    }

    /// Return a copy of this identifier with the given version number instead
    pub fn with_version(&self, version: Version) -> Ident<Self> {
        match self {
            AnyId::Version(v) => Ident::new(v.with_version(version).into_inner().into()),
            AnyId::Build(b) => Ident::new(b.with_version(version).into_inner().into()),
        }
    }

    pub fn build(&self) -> Option<&Build> {
        match self {
            AnyId::Version(_) => None,
            AnyId::Build(BuildId { ref build, .. }) => Some(build),
        }
    }

    pub fn set_build(&mut self, build: Option<Build>) {
        match (self, build) {
            (AnyId::Version(_), None) => {}
            (AnyId::Build(b), Some(build)) => b.set_build(build),
            (id @ AnyId::Version(_), build @ Some(_)) | (id @ AnyId::Build(_), build @ None) => {
                // NOTE(rbottriell): The underlying cloning here is unfortunate, but
                // these cases requires changing the variant and self is &mut. We could
                // do efficient swapping with a little bit of unsafe code by replacing
                // self with an uninitialized value, but that feels like it could properly
                // break async access - TBD if this method could be removed in favor of a
                // better signature or if a little unsafe code is acceptable
                let new = id.with_build(build).into_inner();
                let _ = std::mem::replace(id, new);
            }
        }
    }

    /// Return an identifier for this id with the given build replaced.
    pub fn with_build(&self, build: Option<Build>) -> Ident<AnyId> {
        match (self, build) {
            (AnyId::Version(_), None) => Ident::new(self.clone()),
            (_, Some(build)) => {
                let id = BuildId {
                    name: self.name().to_owned(),
                    version: self.version().clone(),
                    build,
                };
                Ident::new(id.into())
            }
            (_, None) => {
                let id = VersionId {
                    name: self.name().to_owned(),
                    version: self.version().clone(),
                };
                Ident::new(id.into())
            }
        }
    }

    pub fn into_parts(self) -> (PkgNameBuf, Version, Option<Build>) {
        match self {
            Self::Build(BuildId {
                name,
                version,
                build,
            }) => (name, version, Some(build)),
            Self::Version(VersionId { name, version }) => (name, version, None),
        }
    }

    /// Convert into a [`PlacedBuildId`] with the given [`RepositoryNameBuf`].
    ///
    /// A build must be assigned.
    pub fn try_into_placed(self, repository_name: RepositoryNameBuf) -> Result<PlacedBuildId> {
        match self {
            AnyId::Version(_) => Err("Ident must contain a build to become a BuildIdent".into()),
            AnyId::Build(b) => Ok(b.into_placed(repository_name)),
        }
    }
}

impl std::fmt::Display for AnyId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AnyId::Version(inner) => inner.fmt(f),
            AnyId::Build(inner) => inner.fmt(f),
        }
    }
}

impl From<PkgNameBuf> for AnyId {
    fn from(n: PkgNameBuf) -> Self {
        Self::new(n)
    }
}

impl TryFrom<&str> for AnyId {
    type Error = crate::Error;

    fn try_from(value: &str) -> Result<Self> {
        Self::from_str(value)
    }
}

impl TryFrom<&String> for AnyId {
    type Error = crate::Error;

    fn try_from(value: &String) -> Result<Self> {
        Self::from_str(value.as_str())
    }
}

impl TryFrom<String> for AnyId {
    type Error = crate::Error;

    fn try_from(value: String) -> Result<Self> {
        Self::from_str(value.as_str())
    }
}

impl FromStr for AnyId {
    type Err = crate::Error;

    /// Parse the given identifier string into this instance.
    fn from_str(source: &str) -> Result<Self> {
        parsing::any_id::<nom_supreme::error::ErrorTree<_>>(source)
            .map(|(_, ident)| ident)
            .map_err(|err| match err {
                nom::Err::Error(e) | nom::Err::Failure(e) => crate::Error::String(e.to_string()),
                nom::Err::Incomplete(_) => unreachable!(),
            })
    }
}

/// Identifies a package version as a whole, or a recipe.
#[derive(Clone, Debug, Hash, PartialEq, Eq, Ord, PartialOrd)]
pub struct VersionId {
    pub name: PkgNameBuf,
    pub version: Version,
}

impl Named for VersionId {
    fn name(&self) -> &super::PkgName {
        &self.name
    }
}

impl Versioned for VersionId {
    fn version(&self) -> &super::Version {
        &self.version
    }
}

impl VersionedMut for VersionId {
    fn set_version(&mut self, version: super::Version) {
        self.version = version
    }
}

impl std::fmt::Display for VersionId {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.name.fmt(f)?;
        if !self.version.is_zero() {
            f.write_char('/')?;
            self.version.fmt(f)?;
        }
        Ok(())
    }
}

impl VersionId {
    pub fn new(name: PkgNameBuf) -> Self {
        Self {
            name,
            version: Default::default(),
        }
    }

    /// Return a copy of this identifier with the given version number instead
    pub fn with_version(&self, version: Version) -> Ident<Self> {
        Ident::new(Self {
            name: self.name.clone(),
            version,
        })
    }
}

impl MetadataPath for VersionId {
    fn metadata_path(&self) -> RelativePathBuf {
        RelativePathBuf::from(format!("{}/{}", self.name, self.version))
    }
}

impl From<PkgNameBuf> for VersionId {
    fn from(n: PkgNameBuf) -> Self {
        Self::new(n)
    }
}

impl TryFrom<&str> for VersionId {
    type Error = crate::Error;

    fn try_from(value: &str) -> Result<Self> {
        Self::from_str(value)
    }
}

impl TryFrom<&String> for VersionId {
    type Error = crate::Error;

    fn try_from(value: &String) -> Result<Self> {
        Self::from_str(value.as_str())
    }
}

impl TryFrom<String> for VersionId {
    type Error = crate::Error;

    fn try_from(value: String) -> Result<Self> {
        Self::from_str(value.as_str())
    }
}

impl FromStr for VersionId {
    type Err = crate::Error;

    /// Parse the given identifier string into this instance.
    fn from_str(source: &str) -> Result<Self> {
        parsing::version_id::<nom_supreme::error::ErrorTree<_>>(source)
            .map(|(_, ident)| ident)
            .map_err(|err| match err {
                nom::Err::Error(e) | nom::Err::Failure(e) => crate::Error::String(e.to_string()),
                nom::Err::Incomplete(_) => unreachable!(),
            })
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, Ord, PartialOrd)]
pub struct BuildId {
    pub name: PkgNameBuf,
    pub version: Version,
    pub build: Build,
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
    fn build(&self) -> &Build {
        &self.build
    }

    fn set_build(&mut self, build: Build) {
        self.build = build
    }
}

impl std::fmt::Display for BuildId {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.name.fmt(f)?;
        f.write_char('/')?;
        self.version.fmt(f)?;
        f.write_char('/')?;
        self.build.fmt(f)
    }
}

impl BuildId {
    /// Return true if this identifier is for a source package.
    pub fn is_source(&self) -> bool {
        self.build.is_source()
    }

    /// Return a copy of this identifier with the given version number instead
    pub fn with_version(&self, version: Version) -> Ident<Self> {
        Ident::new(Self {
            name: self.name.clone(),
            version,
            build: self.build.clone(),
        })
    }

    /// Return a copy of this identifier with the given build replaced.
    pub fn with_build(&self, build: Build) -> Ident<Self> {
        let mut new = self.clone();
        new.build = build;
        Ident::new(new)
    }

    /// Convert into a [`BuildId`] with the given [`RepositoryNameBuf`].
    ///
    /// A build must be assigned.
    pub fn into_placed(self, repository_name: RepositoryNameBuf) -> PlacedBuildId {
        let Self {
            name,
            version,
            build,
        } = self;
        PlacedBuildId {
            repository_name,
            name,
            version,
            build,
        }
    }
}

impl MetadataPath for BuildId {
    fn metadata_path(&self) -> RelativePathBuf {
        RelativePathBuf::from(self.to_string())
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
        parsing::build_id::<nom_supreme::error::ErrorTree<_>>(source)
            .map(|(_, ident)| ident)
            .map_err(|err| match err {
                nom::Err::Error(e) | nom::Err::Failure(e) => crate::Error::String(e.to_string()),
                nom::Err::Incomplete(_) => unreachable!(),
            })
    }
}

/// Parse a package identifier string.
pub fn parse_ident<S: AsRef<str>>(source: S) -> Result<Ident> {
    AnyId::from_str(source.as_ref()).map(Ident::new)
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
            build: bi.build,
        }
    }
}

impl From<&PlacedBuildId> for BuildId {
    fn from(bi: &PlacedBuildId) -> Self {
        BuildId {
            name: bi.name.clone(),
            version: bi.version.clone(),
            build: bi.build.clone(),
        }
    }
}
