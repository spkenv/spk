// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::{
    cmp::min,
    collections::{BTreeMap, HashSet},
    fmt::{Display, Write},
    str::FromStr,
};

use itertools::Itertools;
use serde::{Deserialize, Serialize};

use super::{
    compat::API_STR, compat::BINARY_STR, parse_build, version_range::Ranged, Build, CompatRule,
    Compatibility, Component, EqualsVersion, Ident, InvalidNameError, PkgName, Spec, Version,
    VersionFilter,
};
use crate::{Error, Result};

#[cfg(test)]
#[path = "./request_test.rs"]
mod request_test;

/// Identifies a range of package versions and builds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RangeIdent {
    pub name: PkgName,
    pub components: HashSet<Component>,
    pub version: VersionFilter,
    pub build: Option<Build>,
}

#[allow(clippy::derive_hash_xor_eq)]
impl std::hash::Hash for RangeIdent {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.name.hash(state);
        self.components.iter().sorted().collect_vec().hash(state);
        self.version.hash(state);
        self.build.hash(state);
    }
}

impl RangeIdent {
    /// Create a range ident that exactly requests the identified package
    ///
    /// The returned range will request the identified components of the given package.
    pub fn exact<I>(ident: &super::Ident, components: I) -> Self
    where
        I: IntoIterator<Item = Component>,
    {
        Self {
            name: ident.name.clone(),
            version: super::VersionFilter::single(
                super::EqualsVersion::from(ident.version.clone()).into(),
            ),
            components: components.into_iter().collect(),
            build: ident.build.clone(),
        }
    }

    pub fn name(&self) -> &str {
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
        if pkg.name != self.name {
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

        if other.build.is_none() || self.build == other.build || self.build.is_none() {
            Compatibility::Compatible
        } else {
            Compatibility::Incompatible(format!("Incompatible builds: {} && {}", self, other))
        }
    }

    pub fn restrict(&mut self, other: &RangeIdent) -> Result<()> {
        if let Err(err) = self.version.restrict(&other.version) {
            return Err(Error::wrap(format!("[{}]", self.name), err));
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
                "Incompatible builds: {} && {}",
                self, other
            )))
        }
    }

    /// Return true if the given package spec satisfies this request.
    pub fn is_satisfied_by(&self, spec: &Spec, required: CompatRule) -> Compatibility {
        if spec.pkg.name != self.name {
            return Compatibility::Incompatible("different package names".into());
        }

        if !self.components.is_empty() && self.build != Some(Build::Source) {
            let required_components = spec.install.components.resolve_uses(self.components.iter());
            let available_components: HashSet<_> = spec
                .install
                .components
                .iter()
                .map(|c| c.name.clone())
                .collect();
            let missing_components = required_components
                .difference(&available_components)
                .sorted()
                .collect_vec();
            if !missing_components.is_empty() {
                return Compatibility::Incompatible(format!(
                    "does not define requested components: [{}], found [{}]",
                    missing_components
                        .into_iter()
                        .map(Component::to_string)
                        .join(", "),
                    available_components
                        .iter()
                        .map(Component::to_string)
                        .sorted()
                        .join(", ")
                ));
            }
        }

        let c = self.version.is_satisfied_by(spec, required);
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
        match self.components.len() {
            0 => (),
            1 => {
                f.write_char(':')?;
                self.components.iter().sorted().join(",").fmt(f)?;
            }
            _ => {
                f.write_char(':')?;
                f.write_char('{')?;
                self.components.iter().sorted().join(",").fmt(f)?;
                f.write_char('}')?;
            }
        }
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
        // Request "alternate" format when serializing, to get, e.g.,
        // "fromBuildEnv: foo/Binary:1.1.2"
        // instead of
        // "fromBuildEnv: foo/b:1.1.2"
        serializer.serialize_str(&format!("{:#}", self))
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
/// spk::api::parse_ident_range("maya/~2020.0").unwrap();
/// spk::api::parse_ident_range("maya/^2020.0").unwrap();
/// ```
pub fn parse_ident_range<S: AsRef<str>>(source: S) -> Result<RangeIdent> {
    let mut parts = source.as_ref().split('/');
    let name_and_components = parts.next().unwrap_or("");
    let (name, components) = parse_name_and_components(name_and_components)?;
    let version = parts.next().unwrap_or("");
    let build = parts.next();

    if parts.next().is_some() {
        return Err(Error::String(format!(
            "Too many tokens in range identifier: {}",
            source.as_ref()
        )));
    }

    Ok(RangeIdent {
        name,
        components,
        version: VersionFilter::from_str(version)?,
        build: match build {
            Some(b) => Some(parse_build(b)?),
            None => None,
        },
    })
}

fn parse_name_and_components<S: AsRef<str>>(source: S) -> Result<(PkgName, HashSet<Component>)> {
    let source = source.as_ref();
    let mut components = HashSet::new();

    if let Some(delim) = source.find(':') {
        let name = source[..delim].parse()?;
        let remainder = &source[delim + 1..];
        let cmpts = match remainder.starts_with('{') {
            true if remainder.ends_with('}') => &remainder[1..remainder.len() - 1],
            true => {
                return Err(InvalidNameError::new_error(
                    "missing or misplaced closing delimeter for component list: '}'".to_string(),
                ))
            }
            false => remainder,
        };

        for cmpt in cmpts.split(',') {
            components.insert(Component::parse(cmpt)?);
        }
        return Ok((name, components));
    }

    Ok((source.parse()?, components))
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord)]
pub enum PreReleasePolicy {
    ExcludeAll,
    IncludeAll,
}

impl PreReleasePolicy {
    pub fn is_default(&self) -> bool {
        matches!(self, &PreReleasePolicy::ExcludeAll)
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
        matches!(self, &InclusionPolicy::Always)
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
#[derive(Debug, Serialize, Clone, Hash, PartialEq, Eq)]
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

    pub fn is_pkg(&self) -> bool {
        matches!(self, Self::Pkg(_))
    }

    pub fn is_var(&self) -> bool {
        matches!(self, Self::Var(_))
    }
}

impl From<VarRequest> for Request {
    fn from(req: VarRequest) -> Self {
        Self::Var(req)
    }
}

impl From<PkgRequest> for Request {
    fn from(req: PkgRequest) -> Self {
        Self::Pkg(req)
    }
}

impl<'de> Deserialize<'de> for Request {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde_yaml::Value;
        let value = Value::deserialize(deserializer)?;
        let mapping = match value {
            Value::Mapping(m) => m,
            _ => return Err(serde::de::Error::custom("expected mapping")),
        };
        if mapping.get(&Value::String("var".to_string())).is_some() {
            Ok(Request::Var(
                VarRequest::deserialize(Value::Mapping(mapping))
                    .map_err(|e| serde::de::Error::custom(format!("{:?}", e)))?,
            ))
        } else if mapping.get(&Value::String("pkg".to_string())).is_some() {
            Ok(Request::Pkg(
                PkgRequest::deserialize(Value::Mapping(mapping))
                    .map_err(|e| serde::de::Error::custom(format!("{:?}", e)))?,
            ))
        } else {
            Err(serde::de::Error::custom(
                "failed to determine request type: must have one of 'var' or 'pkg' fields",
            ))
        }
    }
}

/// A set of restrictions placed on selected packages' build options.
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct VarRequest {
    pub var: String,
    pub pin: bool,
    pub value: String,
}

#[derive(Serialize, Deserialize)]
struct VarRequestSchema {
    var: String,
    #[serde(rename = "fromBuildEnv", default, skip_serializing_if = "is_false")]
    pin: bool,
}

impl VarRequest {
    /// Create a new empty request for the named variable
    pub fn new<S: Into<String>>(name: S) -> Self {
        Self {
            var: name.into(),
            pin: false,
            value: Default::default(),
        }
    }

    /// Create a new request for the named variable at the specified value
    pub fn new_with_value<N, V>(name: N, value: V) -> Self
    where
        N: Into<String>,
        V: Into<String>,
    {
        Self {
            var: name.into(),
            pin: false,
            value: value.into(),
        }
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

    /// The name of this variable without any package qualifier
    pub fn base_name(&self) -> &str {
        let index = self.var.find('.').map(|i| i + 1).unwrap_or_default();
        &self.var[index..]
    }

    /// Return the name of the package that this var refers to (if any)
    pub fn package(&self) -> Option<PkgName> {
        if self.var.contains('.') {
            self.var
                .split('.')
                .next()
                .map(PkgName::from_str)
                .and_then(Result::ok)
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

        let mut parts = spec.var.splitn(2, '/');
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
            (None, true) => (),
            (None, false) => {
                return Err(serde::de::Error::custom(format!(
                    "var request must be in the form name/value, got '{}'",
                    spec.var
                )));
            }
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
        if !self.value.is_empty() || !self.pin {
            // serialize an empty value if not pinning, otherwise it
            // wont be valid to load back in
            var = format!("{}/{}", var, self.value);
        }
        let out = VarRequestSchema { var, pin: self.pin };
        out.serialize(serializer)
    }
}

/// What made a PkgRequest, was it the command line, a test or a
/// package build such as one resolved during a solve, or another
/// package build resolved during a solve.
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum RequestedBy {
    /// From the command line
    CommandLine,
    /// A source package that made the request during a source build resolve
    SourceBuild(Ident),
    /// A package that made the request as part of a binary build env setup
    BinaryBuild(Ident),
    /// A source package that a made the request during a source test
    SourceTest(Ident),
    /// The source package that made the request during a build test
    BuildTest(Ident),
    /// The package that made the request to set up an install test
    InstallTest(Ident),
    /// The request was made for the current environment, so from a
    /// previous spk solve which does not keep past requester data,
    /// and there isn't anymore information
    CurrentEnvironment,
    /// Don't know where what made the request. This is used to cover
    /// a potential error case that should not be possible, but might be.
    Unknown,
    /// For situations when a PkgRequest is created temporarily to use
    /// some of its functionality as part of updating something else
    /// and its "requested by" data is not used in its lifetime,
    /// e.g. temp processing during i/o formatting.
    DoesNotMatter,
    /// For situations when there was no solver state data available
    /// from which to work out what the original merged request was
    /// that resulted in a SetPackage change. This is used to cover a
    /// potential error case that should not be possible.
    NoState,
    /// For a request made during spk's automated (unit) test code
    #[cfg(test)]
    SpkInternalTest,
    /// A package build that made the request, usually during a solve
    PackageBuild(Ident),
}

impl std::fmt::Display for RequestedBy {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            RequestedBy::CommandLine => write!(f, "command line"),
            RequestedBy::SourceBuild(ident) => write!(f, "{ident} source build"),
            RequestedBy::BinaryBuild(ident) => write!(f, "{ident} binary build"),
            RequestedBy::SourceTest(ident) => write!(f, "{ident} source test"),
            RequestedBy::BuildTest(ident) => write!(f, "{ident} build test"),
            RequestedBy::InstallTest(ident) => write!(f, "{ident} install test"),
            RequestedBy::CurrentEnvironment => write!(f, "current environment"),
            RequestedBy::Unknown => write!(f, "unknown"),
            RequestedBy::DoesNotMatter => write!(f, "n/a"),
            RequestedBy::NoState => write!(f, "no state? this should not happen?"),
            #[cfg(test)]
            RequestedBy::SpkInternalTest => write!(f, "spk's test suite"),
            RequestedBy::PackageBuild(ident) => write!(f, "{ident}"),
        }
    }
}

/// A desired package and set of restrictions on how it's selected.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PkgRequest {
    pub pkg: RangeIdent,
    #[serde(
        rename = "prereleasePolicy",
        default,
        skip_serializing_if = "PreReleasePolicy::is_default"
    )]
    pub prerelease_policy: PreReleasePolicy,
    #[serde(
        rename = "include",
        default,
        skip_serializing_if = "InclusionPolicy::is_default"
    )]
    pub inclusion_policy: InclusionPolicy,
    #[serde(
        rename = "fromBuildEnv",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub pin: Option<String>,
    #[serde(skip)]
    pub required_compat: Option<CompatRule>,
    // The 'requested_by' field is a BTreeMap to keep all the
    // requesters grouped by the part of the request they made.
    // Multiple requests are combined into a single merged request
    // during a solve (via restrict()). A merged request will have
    // requesters for each part of the merged request. Merged requests
    // are displayed consistently because of their internal ordering,
    // e.g. gcc/6,8,9, see RangeIdent's version field for details.
    //
    // The BTreeMap retains an ordering that matches that internal
    // ordering by being keyed from string of the part (rule) of the
    // merged request they made. This allows requesters to be
    // retrieved in an order that lines up with the display form of
    // the merged request, and makes it possible produce output
    // messages that align the parts of the request and all the
    // requesters that requested those parts.
    //
    // TODO: consider using the part of the request itself, the rule,
    // as the key in the BTreeMap instead of a String, or use a custom
    // data type that pairs the two things together, something to make
    // the connection between requesters and the parts of requests
    // more approachable.
    #[serde(skip)]
    pub requested_by: BTreeMap<String, Vec<RequestedBy>>,
}

#[allow(clippy::derive_hash_xor_eq)]
impl std::hash::Hash for PkgRequest {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.pkg.hash(state);
        self.prerelease_policy.hash(state);
        self.inclusion_policy.hash(state);
        match &self.pin {
            Some(p) => p.hash(state),
            None => {}
        };
        self.required_compat.hash(state);
        // The 'requested_by' field is not included in the hash
        // because the source(s) of the request shouldn't affect the
        // 'identity' of the request. This should help avoid State bloat.
    }
}

impl PkgRequest {
    pub fn new(pkg: RangeIdent, requester: RequestedBy) -> Self {
        let key = pkg.to_string();
        Self {
            pkg,
            prerelease_policy: PreReleasePolicy::ExcludeAll,
            inclusion_policy: InclusionPolicy::Always,
            pin: Default::default(),
            required_compat: Some(CompatRule::Binary),
            requested_by: BTreeMap::from([(key, vec![requester])]),
        }
    }

    // Sometimes a PkgRequest is created directly without using new()
    // or from_ident() and without knowing what the requester is in
    // the creating function, such as during deserialization
    // (e.g. from parsing command line args, or reading it from the
    // install requirements in build spec file ). This method must be
    // used in those cases to add the requester after the PkgRequest has
    // been created.
    pub fn add_requester(&mut self, requester: RequestedBy) {
        let key = self.pkg.to_string();
        self.requested_by
            .entry(key)
            .or_insert(Vec::new())
            .push(requester);
    }

    /// Return a list of the things that made this request (what that
    /// requested it, what it was requested by)
    pub fn get_requesters(&self) -> Vec<RequestedBy> {
        self.requested_by.values().flatten().cloned().collect()
    }

    // TODO: change parameter to `pkg: Ident`
    pub fn from_ident(pkg: Ident, requester: RequestedBy) -> Self {
        let ri = RangeIdent {
            name: pkg.name,
            components: Default::default(),
            version: VersionFilter::single(EqualsVersion::version_range(pkg.version.clone())),
            build: pkg.build,
        };
        Self::new(ri, requester)
    }

    pub fn with_prerelease(mut self, prerelease_policy: PreReleasePolicy) -> Self {
        self.prerelease_policy = prerelease_policy;
        self
    }

    pub fn with_inclusion(mut self, inclusion_policy: InclusionPolicy) -> Self {
        self.inclusion_policy = inclusion_policy;
        self
    }

    pub fn with_pin(mut self, pin: Option<String>) -> Self {
        self.pin = pin;
        self
    }

    pub fn with_compat(mut self, required_compat: Option<CompatRule>) -> Self {
        self.required_compat = required_compat;
        self
    }

    fn rendered_to_pkgrequest(&self, rendered: Vec<char>) -> Result<PkgRequest> {
        let mut new = self.clone();
        new.pin = None;
        new.pkg.version = VersionFilter::from_str(&rendered.into_iter().collect::<String>())?;
        Ok(new)
    }

    /// Create a copy of this request with it's pin rendered out using 'pkg'.
    pub fn render_pin(&self, pkg: &Ident) -> Result<PkgRequest> {
        match &self.pin {
            None => Err(Error::String(
                "Request has no pin to be rendered".to_owned(),
            )),
            Some(pin) if pin == API_STR || pin == BINARY_STR => {
                // Supply the full base (digit-only) part of the version
                let base = pkg.version.base();
                let mut rendered: Vec<char> = Vec::with_capacity(
                    pin.len()
                        // ':'
                        + 1
                        // version component lengths
                        + base.len(),
                );
                rendered.extend(pin.chars().into_iter());
                rendered.push(':');
                rendered.extend(base.chars().into_iter());
                self.rendered_to_pkgrequest(rendered)
            }
            Some(pin) => {
                let mut digits = pkg.version.parts.iter().chain(std::iter::repeat(&0));
                let mut rendered = Vec::with_capacity(pin.len());
                for char in pin.chars() {
                    if char == 'x' {
                        rendered.extend(digits.next().unwrap().to_string().chars().into_iter());
                    } else {
                        rendered.push(char);
                    }
                }

                self.rendered_to_pkgrequest(rendered)
            }
        }
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

        self.pkg
            .is_satisfied_by(spec, self.required_compat.unwrap_or(CompatRule::Binary))
    }

    /// Reduce the scope of this request to the intersection with another.
    pub fn restrict(&mut self, other: &PkgRequest) -> Result<()> {
        self.prerelease_policy = min(self.prerelease_policy, other.prerelease_policy);
        self.inclusion_policy = min(self.inclusion_policy, other.inclusion_policy);
        self.pkg.restrict(&other.pkg)?;
        // Add the requesters from the other request to this one.
        for (key, request_list) in &other.requested_by {
            for requester in request_list {
                self.requested_by
                    .entry(key.clone())
                    .or_insert(Vec::new())
                    .push(requester.clone());
            }
        }
        Ok(())
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
            pin: Option<serde_yaml::Value>,
        }
        let unchecked = Unchecked::deserialize(deserializer)?;

        // fromBuildEnv can either be a boolean or some other scalar.
        // really only a string makes sense, but some other scalar
        let pin = match unchecked.pin {
            Some(serde_yaml::Value::Bool(b)) => match b {
                true => Some(BINARY_STR.to_string()),
                false => None,
            },
            Some(serde_yaml::Value::String(s)) => Some(s),
            Some(v) => {
                return Err(serde::de::Error::custom(format!(
                    "expected boolean or string value in 'fromBuildEnv', got {:?}",
                    v,
                )));
            }
            None => None,
        };
        if pin.is_some() && !unchecked.pkg.version.is_empty() {
            return Err(serde::de::Error::custom(
                "Package request cannot include both a version number and fromBuildEnv",
            ));
        }
        Ok(Self {
            pkg: unchecked.pkg,
            prerelease_policy: unchecked.prerelease_policy,
            inclusion_policy: unchecked.inclusion_policy,
            pin,
            required_compat: None,
            requested_by: BTreeMap::new(),
        })
    }
}

pub(crate) fn is_false(value: &bool) -> bool {
    !*value
}
