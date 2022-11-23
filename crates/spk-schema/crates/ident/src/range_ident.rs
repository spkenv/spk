// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::BTreeSet;
use std::fmt::Write;
use std::str::FromStr;

use itertools::Itertools;
use serde::{Deserialize, Serialize};
use spk_schema_foundation::ident_build::Build;
use spk_schema_foundation::ident_component::{Component, Components};
use spk_schema_foundation::ident_ops::parsing::KNOWN_REPOSITORY_NAMES;
use spk_schema_foundation::name::{PkgName, PkgNameBuf, RepositoryNameBuf};
use spk_schema_foundation::version::{CompatRule, Compatibility};
use spk_schema_foundation::version_range::{
    DoubleEqualsVersion,
    EqualsVersion,
    Ranged,
    RestrictMode,
    VersionFilter,
    VersionRange,
};

use crate::{AnyIdent, BuildIdent, Error, LocatedBuildIdent, Result, Satisfy, VersionIdent};

#[cfg(test)]
#[path = "./range_ident_test.rs"]
mod range_ident_test;

/// Identifies a range of package versions and builds.
#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub struct RangeIdent {
    pub repository_name: Option<RepositoryNameBuf>,
    pub name: PkgNameBuf,
    pub components: BTreeSet<Component>,
    pub version: VersionFilter,
    pub build: Option<Build>,
}

impl Ord for RangeIdent {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match self.name.cmp(&other.name) {
            std::cmp::Ordering::Equal => {}
            ord => return ord,
        }
        match self
            .components
            .iter()
            .sorted()
            .cmp(other.components.iter().sorted())
        {
            std::cmp::Ordering::Equal => {}
            ord => return ord,
        }
        match self.version.cmp(&other.version) {
            std::cmp::Ordering::Equal => {}
            ord => return ord,
        }
        self.build.cmp(&other.build)
    }
}

impl PartialOrd for RangeIdent {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl RangeIdent {
    fn new<I>(ident: &AnyIdent, version_range: VersionRange, components: I) -> Self
    where
        I: IntoIterator<Item = Component>,
    {
        Self {
            repository_name: None,
            name: ident.name().to_owned(),
            version: VersionFilter::single(version_range),
            components: components.into_iter().collect(),
            build: ident.build().map(Clone::clone),
        }
    }

    /// Create a range ident that requests the identified package using `==` semantics.
    ///
    /// The returned range will request the identified components of the given package.
    pub fn double_equals<I>(ident: &AnyIdent, components: I) -> Self
    where
        I: IntoIterator<Item = Component>,
    {
        Self::new(
            ident,
            DoubleEqualsVersion::from(ident.version().clone()).into(),
            components,
        )
    }

    /// Create a range ident that requests the identified package using `=` semantics.
    ///
    /// The returned range will request the identified components of the given package.
    pub fn equals<I>(ident: &AnyIdent, components: I) -> Self
    where
        I: IntoIterator<Item = Component>,
    {
        Self::new(
            ident,
            EqualsVersion::from(ident.version().clone()).into(),
            components,
        )
    }

    pub fn name(&self) -> &PkgName {
        &self.name
    }

    /// Return true if this ident requests a source package.
    pub fn is_source(&self) -> bool {
        if let Some(build) = &self.build {
            build.is_source()
        } else {
            false
        }
    }

    /// Return true if the given package version is applicable to this range.
    ///
    /// Versions that are applicable are not necessarily satisfactory, but
    /// this cannot be fully determined without a complete package spec.
    pub fn is_applicable(&self, pkg: &AnyIdent) -> bool {
        if pkg.name() != self.name {
            return false;
        }

        if !self.version.is_applicable(pkg.version()).is_ok() {
            return false;
        }

        if self.build.is_some() && self.build.as_ref() != pkg.build() {
            return false;
        }

        true
    }

    pub fn contains(&self, other: &RangeIdent) -> Compatibility {
        if other.name != self.name {
            return Compatibility::incompatible(format!(
                "Version selectors are for different packages: {} != {}",
                self.name, other.name
            ));
        }

        let compat = self.version.contains(&other.version);
        if !compat.is_ok() {
            return compat;
        }

        if other.build.is_none() || self.build == other.build || self.build.is_none() {
            Compatibility::Compatible
        } else {
            Compatibility::incompatible(format!("Incompatible builds: {self} && {other}"))
        }
    }

    /// Reduce this range ident by the scope of another
    ///
    /// This range ident will become restricted to the intersection
    /// of the current version range and the other, as well as
    /// their combined component requests.
    pub fn restrict(&mut self, other: &RangeIdent, mode: RestrictMode) -> Result<()> {
        match (
            self.repository_name.as_ref(),
            other.repository_name.as_ref(),
        ) {
            (None, None) => {}                                 // okay
            (Some(_), None) => {}                              // okay
            (Some(ours), Some(theirs)) if ours == theirs => {} // okay
            (None, rn @ Some(_)) => {
                self.repository_name = rn.cloned();
            }
            (Some(ours), Some(theirs)) => {
                return Err(Error::String(format!(
                    "Incompatible request for package {} from differing repos: {ours} != {theirs}",
                    self.name,
                )))
            }
        };

        if let Err(err) = self.version.restrict(&other.version, mode) {
            return Err(Error::wrap(format!("[{}]", self.name), err.into()));
        }

        for cmpt in other.components.iter() {
            if !self.components.contains(cmpt) {
                self.components.insert(cmpt.clone());
            }
        }

        if other.build.is_none() {
            Ok(())
        } else if self.build == other.build || self.build.is_none() {
            self.build = other.build.clone();
            Ok(())
        } else {
            Err(Error::String(format!(
                "Incompatible builds: {self} && {other}"
            )))
        }
    }

    /// Return true if the given package spec satisfies this request.
    pub fn is_satisfied_by<'a, T>(&'a self, spec: &T, required: CompatRule) -> Compatibility
    where
        T: Satisfy<(&'a RangeIdent, CompatRule)>,
    {
        spec.check_satisfies_request(&(self, required))
    }

    pub fn with_components<I>(self, components: I) -> Self
    where
        I: IntoIterator<Item = Component>,
    {
        Self {
            repository_name: self.repository_name,
            name: self.name,
            version: self.version,
            components: components.into_iter().collect(),
            build: self.build,
        }
    }
}

impl std::fmt::Display for RangeIdent {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        if let Some(name) = &self.repository_name {
            name.fmt(f)?;
            f.write_char('/')?;
        }
        self.name.fmt(f)?;
        self.components.fmt_component_set(f)?;
        if !self.version.is_empty() {
            f.write_char('/')?;
            self.version.fmt(f)?;
        }
        if let Some(build) = &self.build {
            f.write_char('/')?;
            build.fmt(f)?;
        }
        Ok(())
    }
}

impl From<LocatedBuildIdent> for RangeIdent {
    fn from(ident: LocatedBuildIdent) -> Self {
        let (repository_name, build_ident) = ident.into_inner();
        let (version_ident, build) = build_ident.into_inner();
        let (name, version) = version_ident.into_inner();
        Self {
            repository_name: Some(repository_name),
            name,
            version: version.into(),
            components: BTreeSet::default(),
            build: Some(build),
        }
    }
}

impl From<AnyIdent> for RangeIdent {
    fn from(ident: AnyIdent) -> Self {
        let (version_ident, build) = ident.into_inner();
        let (name, version) = version_ident.into_inner();
        Self {
            repository_name: None,
            name,
            version: version.into(),
            components: BTreeSet::default(),
            build,
        }
    }
}

impl From<VersionIdent> for RangeIdent {
    fn from(ident: VersionIdent) -> Self {
        let (name, version) = ident.into_inner();
        Self {
            repository_name: None,
            name,
            version: version.into(),
            components: BTreeSet::default(),
            build: None,
        }
    }
}

impl From<BuildIdent> for RangeIdent {
    fn from(ident: BuildIdent) -> Self {
        let (version_ident, build) = ident.into_inner();
        let (name, version) = version_ident.into_inner();
        Self {
            repository_name: None,
            name,
            version: version.into(),
            components: BTreeSet::default(),
            build: Some(build),
        }
    }
}

impl FromStr for RangeIdent {
    type Err = crate::Error;

    fn from_str(s: &str) -> crate::Result<Self> {
        crate::parsing::range_ident::<nom_supreme::error::ErrorTree<_>>(&KNOWN_REPOSITORY_NAMES, s)
            .map(|(_, ident)| ident)
            .map_err(|err| match err {
                nom::Err::Error(e) | nom::Err::Failure(e) => crate::Error::String(e.to_string()),
                nom::Err::Incomplete(_) => unreachable!(),
            })
    }
}

impl Serialize for RangeIdent {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        // Request "alternate" format when serializing, to get, e.g.,
        // "fromBuildEnv: foo/Binary:1.1.2"
        // instead of
        // "fromBuildEnv: foo/b:1.1.2"
        serializer.serialize_str(&format!("{self:#}"))
    }
}

impl<'de> Deserialize<'de> for RangeIdent {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct RangeIdentVisitor;

        impl<'de> serde::de::Visitor<'de> for RangeIdentVisitor {
            type Value = RangeIdent;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a package and version range (eg: python/3, boost/>=2.5)")
            }

            fn visit_str<E>(self, v: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                parse_ident_range(v).map_err(serde::de::Error::custom)
            }
        }

        deserializer.deserialize_str(RangeIdentVisitor)
    }
}

/// Parse a package identifier which specifies a range of versions.
///
/// ```
/// spk_schema_ident::parse_ident_range("maya/~2020.0").unwrap();
/// spk_schema_ident::parse_ident_range("maya/^2020.0").unwrap();
/// ```
pub fn parse_ident_range<S: AsRef<str>>(source: S) -> Result<RangeIdent> {
    RangeIdent::from_str(source.as_ref())
}
