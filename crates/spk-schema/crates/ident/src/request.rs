// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::cmp::min;
use std::collections::BTreeMap;
use std::fmt::Write;
use std::marker::PhantomData;
use std::str::FromStr;

use colored::Colorize;
use serde::{Deserialize, Serialize};
use spk_schema_foundation::format::{
    FormatBuild,
    FormatChangeOptions,
    FormatComponents,
    FormatRequest,
};
use spk_schema_foundation::ident_component::ComponentSet;
use spk_schema_foundation::name::{OptName, OptNameBuf, PkgName};
use spk_schema_foundation::option_map::Stringified;
use spk_schema_foundation::version::{CompatRule, Compatibility, Version, API_STR, BINARY_STR};
use spk_schema_foundation::version_range::{
    DoubleEqualsVersion,
    EqualsVersion,
    Ranged,
    RestrictMode,
    VersionFilter,
};

use super::AnyIdent;
use crate::{BuildIdent, Error, RangeIdent, Result, Satisfy, VersionIdent};

#[cfg(test)]
#[path = "./request_test.rs"]
mod request_test;

#[derive(
    Debug, Clone, Copy, Hash, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord, Default,
)]
pub enum PreReleasePolicy {
    #[default]
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
        f.write_fmt(format_args!("{self:?}"))
    }
}

impl std::str::FromStr for PreReleasePolicy {
    type Err = crate::Error;
    fn from_str(value: &str) -> crate::Result<Self> {
        serde_yaml::from_str(value).map_err(Error::InvalidPreReleasePolicy)
    }
}

#[derive(
    Debug, Clone, Copy, Hash, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord, Default,
)]
pub enum InclusionPolicy {
    #[default]
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
        f.write_fmt(format_args!("{self:?}"))
    }
}

impl std::str::FromStr for InclusionPolicy {
    type Err = crate::Error;
    fn from_str(value: &str) -> crate::Result<Self> {
        serde_yaml::from_str(value).map_err(Error::InvalidInclusionPolicy)
    }
}

/// Represents a constraint added to a resolved environment.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(untagged)]
pub enum Request {
    Pkg(PkgRequest),
    Var(VarRequest),
}

impl Request {
    /// Return the canonical name of this request."""
    pub fn name(&self) -> &OptName {
        match self {
            Request::Var(r) => &r.var,
            Request::Pkg(r) => r.pkg.name.as_opt_name(),
        }
    }

    pub fn is_pkg(&self) -> bool {
        matches!(self, Self::Pkg(_))
    }

    pub fn into_pkg(self) -> Option<PkgRequest> {
        match self {
            Self::Pkg(p) => Some(p),
            _ => None,
        }
    }

    pub fn is_var(&self) -> bool {
        matches!(self, Self::Var(_))
    }

    pub fn into_var(self) -> Option<VarRequest> {
        match self {
            Self::Var(v) => Some(v),
            _ => None,
        }
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
        /// This visitor captures all fields that could be valid
        /// for any request, before deciding at the end which variant
        /// to actually build. We ignore any unrecognized field anyway,
        /// but additionally any field that's recognized must be valid
        /// even if it's not going to be used.
        ///
        /// The purpose of this setup is to enable more meaningful errors
        /// for invalid values that contain original source positions. In
        /// order to achieve this we must parse and validate each field with
        /// the appropriate type as they are visited - which disqualifies the
        /// existing approach to untagged enums which read all fields first
        /// and then goes back and checks them once the variant is determined
        #[derive(Default)]
        struct RequestVisitor {
            // PkgRequest
            pkg: Option<RangeIdent>,
            prerelease_policy: Option<PreReleasePolicy>,
            inclusion_policy: Option<InclusionPolicy>,

            // VarRequest
            var: Option<OptNameBuf>,
            value: Option<String>,

            // Both
            pin: Option<PinValue>,
        }

        impl<'de> serde::de::Visitor<'de> for RequestVisitor {
            type Value = Request;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a pkg or var request")
            }

            fn visit_map<A>(mut self, mut map: A) -> std::result::Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                while let Some(key) = map.next_key::<Stringified>()? {
                    match key.as_str() {
                        "pkg" => self.pkg = Some(map.next_value::<RangeIdent>()?),
                        "prereleasePolicy" => {
                            self.prerelease_policy = Some(map.next_value::<PreReleasePolicy>()?)
                        }
                        "include" => {
                            self.inclusion_policy = Some(map.next_value::<InclusionPolicy>()?)
                        }
                        "fromBuildEnv" => self.pin = Some(map.next_value::<PinValue>()?),
                        "var" => {
                            let NameAndValue(name, value) = map.next_value()?;
                            self.var = Some(name);
                            self.value = value;
                        }
                        "value" => self.value = Some(map.next_value::<String>()?),
                        _ => {
                            // unrecognized fields are explicitly ignored in case
                            // they were added in a newer version of spk. We assume
                            // that if the api has not been versioned then the desire
                            // is to continue working in this older version
                            map.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }

                match (self.pkg, self.var) {
                    (Some(pkg), None) if self.pin.as_ref().map(PinValue::is_some).unwrap_or_default() && !pkg.version.is_empty() => {
                        Err(serde::de::Error::custom(
                            format!("request for `{}` cannot specify a value `/{}` when `fromBuildEnv` is specified", pkg.name, pkg.version)
                        ))
                    },
                    (Some(pkg), None) => Ok(Request::Pkg(PkgRequest {
                        pkg,
                        prerelease_policy: self.prerelease_policy.unwrap_or_default(),
                        inclusion_policy: self.inclusion_policy.unwrap_or_default(),
                        pin: self.pin.unwrap_or_default().into_pkg_pin(),
                        required_compat: None,
                        requested_by: Default::default(),
                    })),
                    (None, Some(var)) => {
                        match self.value.as_ref() {
                            Some(value) if self.pin.as_ref().map(PinValue::is_some).unwrap_or_default() => {
                                Err(serde::de::Error::custom(
                                    format!("request for `{var}` cannot specify a value `/{value}` when `fromBuildEnv` is true")
                                ))
                            }
                            None if self.pin.is_none() => Err(serde::de::Error::custom(
                                format!("request for `{var}` must specify a value (eg: {var}/<value>) when `fromBuildEnv` is false or omitted")
                            )),
                            _ => Ok(Request::Var(VarRequest {
                                var,
                                pin: self.pin.unwrap_or_default().into_var_pin()?,
                                value: self.value.unwrap_or_default(),
                            }))
                        }
                    },
                    (Some(_), Some(_)) => Err(serde::de::Error::custom(
                        "could not determine request type, it may only contain one of the `pkg` or `var` fields"
                    )),
                    (None, None) => Err(serde::de::Error::custom(
                        "could not determine request type, it must include either a `pkg` or `var` field"
                    )
                    ),
                }
            }
        }

        deserializer.deserialize_any(RequestVisitor::default())
    }
}

/// A set of restrictions placed on selected packages' build options.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct VarRequest {
    pub var: OptNameBuf,
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
    pub fn new<S: Into<OptNameBuf>>(name: S) -> Self {
        Self {
            var: name.into(),
            pin: false,
            value: Default::default(),
        }
    }

    /// Create a new request for the named variable at the specified value
    pub fn new_with_value<N, V>(name: N, value: V) -> Self
    where
        N: Into<OptNameBuf>,
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

    /// Check if this package spec satisfies the given var request.
    pub fn is_satisfied_by<T>(&self, spec: &T) -> Compatibility
    where
        T: Satisfy<VarRequest>,
    {
        spec.check_satisfies_request(self)
    }
}

impl Serialize for VarRequest {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut var = self.var.to_string();
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
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum RequestedBy {
    /// From the command line
    CommandLine,
    /// Embedded in another package
    Embedded(BuildIdent),
    /// A source package that made the request during a source build resolve
    SourceBuild(AnyIdent),
    /// A package that made the request as part of a binary build env setup
    BinaryBuild(BuildIdent),
    /// A source package that a made the request during a source test
    SourceTest(AnyIdent),
    /// The source package that made the request during a build test
    BuildTest(AnyIdent),
    /// The package that made the request to set up an install test
    InstallTest(VersionIdent),
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
    SpkInternalTest,
    /// A package build that made the request, usually during a solve
    PackageBuild(BuildIdent),
    /// A package version/recipe that made the request as part of
    /// building from source during a solve
    PackageVersion(VersionIdent),
}

impl std::fmt::Display for RequestedBy {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            RequestedBy::CommandLine => write!(f, "command line"),
            RequestedBy::Embedded(ident) => write!(f, "embedded in {ident}"),
            RequestedBy::SourceBuild(ident) => write!(f, "{ident} source build"),
            RequestedBy::BinaryBuild(ident) => write!(f, "{ident} binary build"),
            RequestedBy::SourceTest(ident) => write!(f, "{ident} source test"),
            RequestedBy::BuildTest(ident) => write!(f, "{ident} build test"),
            RequestedBy::InstallTest(ident) => write!(f, "{ident} install test"),
            RequestedBy::CurrentEnvironment => write!(f, "current environment"),
            RequestedBy::Unknown => write!(f, "unknown"),
            RequestedBy::DoesNotMatter => write!(f, "n/a"),
            RequestedBy::NoState => write!(f, "no state? this should not happen?"),
            RequestedBy::SpkInternalTest => write!(f, "spk's test suite"),
            RequestedBy::PackageBuild(ident) => write!(f, "{ident}"),
            RequestedBy::PackageVersion(ident) => write!(f, "{ident} recipe"),
        }
    }
}

/// A desired package and set of restrictions on how it's selected.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
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

impl std::fmt::Display for PkgRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if f.alternate() {
            let fmt = self.format_request(&None, &self.pkg.name, &FormatChangeOptions::default());
            f.write_str(&fmt)
        } else {
            self.pkg.fmt(f)
        }
    }
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
    // The level/depth for initial requests
    pub const INITIAL_REQUESTS_LEVEL: u64 = 0;
    // Show request fields that are non-default values at v > 1
    pub const SHOW_REQUEST_DETAILS: u32 = 1;
    // Show all request fields for initial requests at v > 5
    pub const SHOW_INITIAL_REQUESTS_FULL_VALUES: u32 = 5;

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
        self.requested_by.entry(key).or_default().push(requester);
    }

    /// Return a list of the things that made this request (what that
    /// requested it, what it was requested by)
    pub fn get_requesters(&self) -> Vec<RequestedBy> {
        self.requested_by.values().flatten().cloned().collect()
    }

    pub fn from_ident(pkg: AnyIdent, requester: RequestedBy) -> Self {
        let (version_ident, build) = pkg.into_inner();
        let (name, version) = version_ident.into_inner();
        let ri = RangeIdent {
            repository_name: None,
            name,
            components: Default::default(),
            version: VersionFilter::single(EqualsVersion::version_range(version)),
            build,
        };
        Self::new(ri, requester)
    }

    pub fn from_ident_exact(pkg: AnyIdent, requester: RequestedBy) -> Self {
        let (version_ident, build) = pkg.into_inner();
        let (name, version) = version_ident.into_inner();
        let ri = RangeIdent {
            repository_name: None,
            name,
            components: Default::default(),
            version: VersionFilter::single(DoubleEqualsVersion::version_range(version)),
            build,
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
    pub fn render_pin(&self, pkg: &BuildIdent) -> Result<PkgRequest> {
        match &self.pin {
            None => Err(Error::String(
                "Request has no pin to be rendered".to_owned(),
            )),
            Some(pin) if pin == API_STR || pin == BINARY_STR => {
                // Supply the full base (digit-only) part of the version
                let base = pkg.version().base();
                let mut rendered: Vec<char> = Vec::with_capacity(
                    pin.len()
                        // ':'
                        + 1
                        // version component lengths
                        + base.len(),
                );
                rendered.extend(pin.chars());
                rendered.push(':');
                rendered.extend(base.chars());
                self.rendered_to_pkgrequest(rendered)
            }
            Some(pin) => {
                let mut digits = pkg.version().parts.iter().chain(std::iter::repeat(&0));
                let mut rendered = Vec::with_capacity(pin.len());
                for char in pin.chars() {
                    if char == 'x' {
                        rendered.extend(digits.next().unwrap().to_string().chars());
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

    /// Return true if the given item satisfies this request.
    pub fn is_satisfied_by<T>(&self, satisfy: &T) -> Compatibility
    where
        T: Satisfy<Self>,
    {
        satisfy.check_satisfies_request(self)
    }

    /// Reduce the scope of this request to the intersection with another.
    pub fn restrict(&mut self, other: &PkgRequest) -> Result<()> {
        self.prerelease_policy = min(self.prerelease_policy, other.prerelease_policy);
        self.inclusion_policy = min(self.inclusion_policy, other.inclusion_policy);
        // Allow otherwise impossible to satisfy combinations of requests
        // to be merged if the combined inclusion policy is `IfAlreadyPresent`.
        //
        // Example: pkg-name/=1.0 && pkg-name/=2.0
        //
        // The solve may find a solution without needing to satisfy this request
        // at all, if the package never becomes "present". By contrast, if this
        // combination is rejected, then it might reject the only possible state
        // that leads to a solution.
        //
        // This behavior is trading performance for correctness. The solver will
        // have to explore a larger search space because of this, to be correct
        // in pathological cases, when it might arrive at a good solution earlier
        // if it were to reject these types of combinations.
        let version_range_restrict_mode =
            if self.inclusion_policy == InclusionPolicy::IfAlreadyPresent {
                RestrictMode::AllowNonIntersectingRanges
            } else {
                RestrictMode::RequireIntersectingRanges
            };
        self.pkg.restrict(&other.pkg, version_range_restrict_mode)?;
        // Add the requesters from the other request to this one.
        for (key, request_list) in &other.requested_by {
            for requester in request_list {
                self.requested_by
                    .entry(key.clone())
                    .or_default()
                    .push(requester.clone());
            }
        }
        Ok(())
    }
}

impl FormatRequest for PkgRequest {
    type PkgRequest = Self;

    fn format_request(
        &self,
        repository_name: &Option<spk_schema_foundation::name::RepositoryNameBuf>,
        name: &PkgName,
        format_settings: &spk_schema_foundation::format::FormatChangeOptions,
    ) -> String {
        let mut out = match repository_name {
            Some(repository_name) => format!("{repository_name}/{}", name.as_str().bold()),
            None => name.as_str().bold().to_string(),
        };
        let mut versions = Vec::new();
        let mut components = ComponentSet::new();
        let mut version = self.pkg.version.to_string();
        if version.is_empty() {
            version.push('*')
        }
        let build = match self.pkg.build {
            Some(ref b) => format!("/{}", b.format_build()),
            None => "".to_string(),
        };

        let details = if format_settings.verbosity > Self::SHOW_REQUEST_DETAILS
            || format_settings.level == Self::INITIAL_REQUESTS_LEVEL
        {
            let mut differences = Vec::new();
            let show_full_value = format_settings.level == Self::INITIAL_REQUESTS_LEVEL
                && format_settings.verbosity > Self::SHOW_INITIAL_REQUESTS_FULL_VALUES;

            if show_full_value || !self.prerelease_policy.is_default() {
                differences.push(format!(
                    "PreReleasePolicy: {}",
                    self.prerelease_policy.to_string().cyan()
                ));
            }
            if show_full_value || !self.inclusion_policy.is_default() {
                differences.push(format!(
                    "InclusionPolicy: {}",
                    self.inclusion_policy.to_string().cyan()
                ));
            }
            if let Some(pin) = &self.pin {
                differences.push(format!("fromBuildEnv: {}", pin.to_string().cyan()));
            }
            if let Some(rc) = self.required_compat {
                let req_compat = format!("{rc:#}");
                differences.push(format!("RequiredCompat: {}", req_compat.cyan()));
            };

            if differences.is_empty() {
                "".to_string()
            } else {
                format!(" ({})", differences.join(", "))
            }
        } else {
            "".to_string()
        };

        versions.push(format!("{}{}{}", version.bright_blue(), build, details));
        components.extend(&mut self.pkg.components.iter().cloned());

        if !components.is_empty() {
            let _ = write!(out, ":{}", components.format_components().dimmed());
        }
        out.push('/');
        out.push_str(&versions.join(","));
        out
    }
}

pub fn is_false(value: &bool) -> bool {
    !*value
}

/// A deserializable name and optional value where
/// the value it identified by it's position following
/// a forward slash (eg: `/<value>`)
pub struct NameAndValue<Name = OptNameBuf>(pub Name, pub Option<String>)
where
    Name: FromStr,
    <Name as FromStr>::Err: std::fmt::Display;

impl<'de, Name> Deserialize<'de> for NameAndValue<Name>
where
    Name: FromStr,
    <Name as FromStr>::Err: std::fmt::Display,
{
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct NameAndValueVisitor<Name>(PhantomData<dyn Fn() -> Name>)
        where
            Name: FromStr,
            <Name as FromStr>::Err: std::fmt::Display;

        impl<'de, Name> serde::de::Visitor<'de> for NameAndValueVisitor<Name>
        where
            Name: FromStr,
            <Name as FromStr>::Err: std::fmt::Display,
        {
            type Value = (Name, Option<String>);

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a var name an optional value (eg, `my-var`, `my-var/value`)")
            }

            fn visit_str<E>(self, v: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                let mut parts = v.splitn(2, '/');
                let name = parts
                    .next()
                    .unwrap()
                    .parse()
                    .map_err(serde::de::Error::custom)?;
                Ok((name, parts.next().map(String::from)))
            }
        }

        deserializer
            .deserialize_str(NameAndValueVisitor::<Name>(PhantomData))
            .map(|(n, v)| NameAndValue(n, v))
    }
}

/// An ambiguous pin value that could be for either a var or
/// pkg request. It represents all the possible values of both,
/// and so may not be valid depending on the final context
enum PinValue {
    None,
    True,
    String(String),
}

impl Default for PinValue {
    fn default() -> Self {
        Self::None
    }
}

impl PinValue {
    /// Transform this pin into the appropriate value for a pkg request
    fn into_pkg_pin(self) -> Option<String> {
        match self {
            Self::None => None,
            Self::True => Some(BINARY_STR.into()),
            Self::String(s) => Some(s),
        }
    }

    /// Transform this pin into the appropriate value for a var request, if possible
    fn into_var_pin<E>(self) -> std::result::Result<bool, E>
    where
        E: serde::de::Error,
    {
        match self {
            Self::None => Ok(false),
            Self::True => Ok(true),
            Self::String(s) => Err(E::custom(format!(
                "`fromBuildEnv` for var requests must be a boolean, found `{s}`"
            ))),
        }
    }

    /// True if this pin has a value
    fn is_some(&self) -> bool {
        !matches!(self, Self::None)
    }
}

impl<'de> Deserialize<'de> for PinValue {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct PinValueVisitor;

        impl<'de> serde::de::Visitor<'de> for PinValueVisitor {
            type Value = PinValue;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a string or boolean (eg, `true`, `Binary`, `x.x.x`)")
            }

            fn visit_bool<E>(self, v: bool) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match v {
                    true => Ok(PinValue::True),
                    false => Ok(PinValue::None),
                }
            }

            fn visit_str<E>(self, v: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(PinValue::String(v.into()))
            }
        }

        deserializer.deserialize_any(PinValueVisitor)
    }
}
