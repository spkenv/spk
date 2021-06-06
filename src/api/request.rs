// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::{
    cmp::min,
    fmt::{Display, Write},
    str::FromStr,
};

use pyo3::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{Error, Result};

use super::{
    parse_build, validate_name, version_range::Ranged, Build, CompatRule, Compatibility,
    ExactVersion, Ident, Spec, Version, VersionFilter,
};

#[cfg(test)]
#[path = "./request_test.rs"]
mod request_test;

/// Identitfies a range of package versions and builds.
#[pyclass]
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct RangeIdent {
    #[pyo3(get)]
    name: String,
    #[pyo3(get, set)]
    pub version: VersionFilter,
    #[pyo3(get, set)]
    pub build: Option<Build>,
}

impl RangeIdent {
    pub fn name<'a>(&'a self) -> &'a str {
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
    pub fn is_applicable(&self, pkg: &Ident) -> bool {
        if pkg.name() != self.name {
            return false;
        }

        if !self.version.is_applicable(&pkg.version).is_ok() {
            return false;
        }

        if self.build.is_some() && self.build != pkg.build {
            return false;
        }

        true
    }

    pub fn contains(&self, other: &RangeIdent) -> Compatibility {
        if other.name != self.name {
            return Compatibility::Incompatible(format!(
                "Version selectors are for different packages: {} != {}",
                self.name, other.name
            ));
        }

        let compat = self.version.contains(&other.version);
        if !compat.is_ok() {
            return compat;
        }

        if other.build.is_none() {
            Compatibility::Compatible
        } else if self.build == other.build || self.build.is_none() {
            Compatibility::Compatible
        } else {
            Compatibility::Incompatible(format!("Incompatible builds: {} && {}", self, other))
        }
    }

    pub fn restrict(&mut self, other: &RangeIdent) -> Result<()> {
        if let Err(err) = self.version.restrict(&other.version) {
            return Err(Error::wrap(format!("{:?} [{}]", err, self.name), err));
        }

        if other.build.is_none() {
            Ok(())
        } else if self.build == other.build || self.build.is_none() {
            self.build = other.build.clone();
            Ok(())
        } else {
            Err(Error::String(format!(
                "Incompatible builds: {} && {}",
                self, other
            )))
        }
    }
}

#[pymethods]
impl RangeIdent {
    /// Return true if the given package spec satisfies this request.
    pub fn is_satisfied_by(&self, spec: &Spec, required: CompatRule) -> Compatibility {
        if spec.pkg.name() != self.name {
            return Compatibility::Incompatible("different package names".into());
        }

        let c = self.version.is_satisfied_by(&spec, required);
        if !c.is_ok() {
            return c;
        }

        if self.build.is_some() && self.build != spec.pkg.build {
            return Compatibility::Incompatible(format!(
                "requested build {:?} != {:?}",
                self.build, spec.pkg.build
            ));
        }

        Compatibility::Compatible
    }
}

impl Display for RangeIdent {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.name.fmt(f)?;
        if self.version.len() > 0 {
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

impl FromStr for RangeIdent {
    type Err = crate::Error;

    fn from_str(s: &str) -> crate::Result<Self> {
        parse_ident_range(s)
    }
}

impl Serialize for RangeIdent {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for RangeIdent {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let ident = String::deserialize(deserializer)?;
        match parse_ident_range(ident) {
            Err(err) => Err(serde::de::Error::custom(format!("{}", err))),
            Ok(ident) => Ok(ident),
        }
    }
}

/// Parse a package identifier which specifies a range of versions.
///
/// ```
/// parse_ident_range("maya/~2020.0").unwrap()
/// parse_ident_range("maya/^2020.0").unwrap()
/// ```
pub fn parse_ident_range<S: AsRef<str>>(source: S) -> Result<RangeIdent> {
    let mut parts = source.as_ref().split("/");
    let name = match parts.next() {
        Some(s) => s,
        None => "",
    };
    let version = match parts.next() {
        Some(s) => s,
        None => "",
    };
    let build = parts.next();

    if let Some(_) = parts.next() {
        return Err(Error::String(format!(
            "Too many tokens in range identifier: {}",
            source.as_ref()
        )));
    }

    validate_name(name)?;
    Ok(RangeIdent {
        name: name.to_string(),
        version: VersionFilter::from_str(version)?,
        build: match build {
            Some(b) => Some(parse_build(b)?),
            None => None,
        },
    })
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord)]
pub enum PreReleasePolicy {
    ExcludeAll,
    IncludeAll,
}

impl PreReleasePolicy {
    pub fn is_default(&self) -> bool {
        if let PreReleasePolicy::ExcludeAll = self {
            true
        } else {
            false
        }
    }
}

impl std::fmt::Display for PreReleasePolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_fmt(format_args!("{:?}", self))
    }
}

impl std::str::FromStr for PreReleasePolicy {
    type Err = crate::Error;
    fn from_str(value: &str) -> crate::Result<Self> {
        Ok(serde_yaml::from_str(value)?)
    }
}

impl Default for PreReleasePolicy {
    fn default() -> Self {
        PreReleasePolicy::ExcludeAll
    }
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord)]
pub enum InclusionPolicy {
    Always,
    IfAlreadyPresent,
}

impl InclusionPolicy {
    pub fn is_default(&self) -> bool {
        if let InclusionPolicy::Always = self {
            true
        } else {
            false
        }
    }
}

impl std::fmt::Display for InclusionPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_fmt(format_args!("{:?}", self))
    }
}

impl std::str::FromStr for InclusionPolicy {
    type Err = crate::Error;
    fn from_str(value: &str) -> crate::Result<Self> {
        Ok(serde_yaml::from_str(value)?)
    }
}

impl Default for InclusionPolicy {
    fn default() -> Self {
        InclusionPolicy::Always
    }
}

/// Represents a contraint added to a resolved environment.
#[derive(Debug, Deserialize, Serialize, Clone, Hash, PartialEq, Eq, FromPyObject)]
#[serde(untagged)]
pub enum Request {
    Var(VarRequest),
    Pkg(PkgRequest),
}

impl Request {
    /// Return the canonical name of this requirement."""
    pub fn name(&self) -> String {
        match self {
            Request::Var(r) => r.var.to_owned(),
            Request::Pkg(r) => r.pkg.to_string(),
        }
    }
}

/// A set of restrictions placed on selected packages' build options.
#[pyclass]
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct VarRequest {
    #[pyo3(get, set)]
    pub var: String,
    #[pyo3(get, set)]
    pub pin: bool,
    #[pyo3(get)]
    pub value: String,
}

#[derive(Serialize, Deserialize)]
struct VarRequestSchema {
    var: String,
    #[serde(rename = "fromBuildEnv", default, skip_serializing_if = "is_false")]
    pin: bool,
}

impl VarRequest {
    pub fn new<S: AsRef<str>>(name: S) -> Self {
        Self {
            var: name.as_ref().to_string(),
            pin: false,
            value: Default::default(),
        }
    }

    pub fn value<'a>(&'a self) -> &'a str {
        self.value.as_str()
    }

    /// Create a copy of this request with it's pin rendered out using 'var'.
    pub fn render_pin<S: Into<String>>(&self, value: S) -> Result<VarRequest> {
        if !self.pin {
            return Err(Error::String(
                "Request has no pin to be rendered".to_string(),
            ));
        }

        let mut new = self.clone();
        new.pin = false;
        new.value = value.into();
        Ok(new)
    }
}

#[pymethods]
impl VarRequest {
    #[new]
    #[args(value = "\"\"")]
    fn init(var: &str, value: &str) -> Self {
        let mut r = Self::new(var);
        r.value = value.to_string();
        r
    }

    /// Return the name of the package that this var refers to (if any)
    pub fn package(&self) -> Option<String> {
        if self.var.contains(".") {
            Some(self.var.split(".").next().unwrap().to_string())
        } else {
            None
        }
    }
}

impl<'de> Deserialize<'de> for VarRequest {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let spec = VarRequestSchema::deserialize(deserializer)?;

        if !spec.var.contains("/") && !spec.pin {
            return Err(serde::de::Error::custom(format!(
                "var request must be in the form name/value, got '{}'",
                spec.var
            )));
        }

        let mut parts = spec.var.splitn(2, "/");
        let mut out = Self {
            var: parts.next().unwrap().to_string(),
            value: Default::default(),
            pin: spec.pin,
        };
        match (parts.next(), spec.pin) {
            (Some(_), true) => {
                return Err(serde::de::Error::custom(format!(
                    "var request {} cannot have value when fromBuildEnv is true",
                    out.var
                )));
            }
            (Some(value), false) => out.value = value.to_string(),
            (None, _) => (),
        }

        Ok(out)
    }
}

impl Serialize for VarRequest {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut var = self.var.clone();
        if self.value != "" {
            var = format!("{}/{}", var, self.value);
        }
        let out = VarRequestSchema {
            var: var,
            pin: self.pin,
        };
        out.serialize(serializer)
    }
}

/// A desired package and set of restrictions on how it's selected.
#[pyclass]
#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize)]
pub struct PkgRequest {
    #[pyo3(get, set)]
    pub pkg: RangeIdent,
    #[serde(
        rename = "prereleasePolicy",
        default,
        skip_serializing_if = "PreReleasePolicy::is_default"
    )]
    #[pyo3(get, set)]
    pub prerelease_policy: PreReleasePolicy,
    #[serde(
        rename = "include",
        default,
        skip_serializing_if = "InclusionPolicy::is_default"
    )]
    #[pyo3(get, set)]
    pub inclusion_policy: InclusionPolicy,
    #[serde(
        rename = "fromBuildEnv",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    #[pyo3(get, set)]
    pub pin: Option<String>,
    #[serde(skip)]
    #[pyo3(get, set)]
    pub required_compat: CompatRule,
}

impl PkgRequest {
    pub fn new(pkg: RangeIdent) -> Self {
        Self {
            pkg: pkg,
            prerelease_policy: PreReleasePolicy::ExcludeAll,
            inclusion_policy: InclusionPolicy::Always,
            pin: Default::default(),
            required_compat: CompatRule::ABI,
        }
    }

    /// Create a copy of this request with it's pin rendered out using 'pkg'.
    pub fn render_pin(&self, pkg: &Ident) -> Result<PkgRequest> {
        match &self.pin {
            None => {
                return Err(Error::String(
                    "Request has no pin to be rendered".to_owned(),
                ))
            }
            Some(pin) => {
                let mut digits = pkg.version.parts().into_iter().chain(std::iter::repeat(0));
                let mut rendered = Vec::with_capacity(pin.len());
                for char in pin.chars() {
                    if char == 'x' {
                        rendered.extend(digits.next().unwrap().to_string().chars().into_iter());
                    } else {
                        rendered.push(char);
                    }
                }

                let mut new = self.clone();
                new.pin = None;
                new.pkg.version =
                    VersionFilter::from_str(&rendered.into_iter().collect::<String>())?;
                Ok(new)
            }
        }
    }
}

#[pymethods]
impl PkgRequest {
    #[new]
    fn init(pkg: RangeIdent, prerelease_policy: Option<PreReleasePolicy>) -> Self {
        let mut req = Self::new(pkg);
        if let Some(prp) = prerelease_policy {
            req.prerelease_policy = prp
        }
        req
    }

    fn copy(&self) -> Self {
        self.clone()
    }

    ///Return true if the given version number is applicable to this request.
    ///
    /// This is used a cheap preliminary way to prune package
    /// versions that are not going to satisfy the request without
    /// needing to load the whole package spec.
    pub fn is_version_applicable(&self, version: &Version) -> Compatibility {
        if self.prerelease_policy == PreReleasePolicy::ExcludeAll && !version.pre.is_empty() {
            Compatibility::Incompatible("prereleases not allowed".to_owned())
        } else {
            self.pkg.version.is_applicable(version)
        }
    }

    /// Return true if the given package spec satisfies this request.
    pub fn is_satisfied_by(&self, spec: &Spec) -> Compatibility {
        if spec.deprecated {
            // deprecated builds are only okay if their build
            // was specifically requested
            if self.pkg.build.is_none() || self.pkg.build != spec.pkg.build {
                return Compatibility::Incompatible(
                    "Build is deprecated and was not specifically requested".to_string(),
                );
            }
        }

        if self.prerelease_policy == PreReleasePolicy::ExcludeAll
            && !spec.pkg.version.pre.is_empty()
        {
            return Compatibility::Incompatible("prereleases not allowed".to_string());
        }

        return self.pkg.is_satisfied_by(spec, self.required_compat);
    }

    /// Reduce the scope of this request to the intersection with another.
    pub fn restrict(&mut self, other: &PkgRequest) -> Result<()> {
        self.prerelease_policy = min(self.prerelease_policy, other.prerelease_policy);
        self.inclusion_policy = min(self.inclusion_policy, other.inclusion_policy);
        self.pkg.restrict(&other.pkg)
    }

    #[staticmethod]
    fn from_ident(pkg: &Ident) -> Self {
        Self::from(pkg)
    }
}

impl From<&Ident> for PkgRequest {
    fn from(pkg: &Ident) -> PkgRequest {
        let ri = RangeIdent {
            name: pkg.name().to_owned(),
            version: VersionFilter::single(ExactVersion::new(pkg.version.clone())),
            build: pkg.build.clone(),
        };
        PkgRequest::new(ri)
    }
}

impl<'de> Deserialize<'de> for PkgRequest {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Unchecked {
            pkg: RangeIdent,
            #[serde(rename = "prereleasePolicy", default)]
            prerelease_policy: PreReleasePolicy,
            #[serde(rename = "include", default)]
            inclusion_policy: InclusionPolicy,
            #[serde(rename = "fromBuildEnv", default)]
            pin: Option<String>,
        }
        let unchecked = Unchecked::deserialize(deserializer)?;
        if unchecked.pin.is_some() && !unchecked.pkg.version.is_empty() {
            return Err(serde::de::Error::custom(
                "Package request cannot include both a version number and fromBuildEnv",
            ));
        }
        Ok(Self {
            pkg: unchecked.pkg,
            prerelease_policy: unchecked.prerelease_policy,
            inclusion_policy: unchecked.inclusion_policy,
            pin: unchecked.pin,
            required_compat: CompatRule::ABI,
        })
    }
}

pub(crate) fn is_false(value: &bool) -> bool {
    *value
}
