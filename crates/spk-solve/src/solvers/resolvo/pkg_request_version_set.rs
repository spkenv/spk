// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::sync::Arc;

use resolvo::utils::VersionSet;
use spk_schema::Request;
use spk_schema::ident::{LocatedBuildIdent, PkgRequest, PreReleasePolicy, RangeIdent, RequestedBy};
use spk_schema::ident_component::Component;

/// This allows for storing strings of different types but hash and compare by
/// the underlying strings.
#[derive(Clone, Debug)]
pub(crate) enum VarValue {
    ArcStr(Arc<str>),
    Owned(String),
}

impl VarValue {
    #[inline]
    fn as_str(&self) -> &str {
        match self {
            VarValue::ArcStr(a) => a,
            VarValue::Owned(a) => a.as_str(),
        }
    }
}

impl std::hash::Hash for VarValue {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.as_str().hash(state)
    }
}

impl Eq for VarValue {}

impl Ord for VarValue {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.as_str().cmp(other.as_str())
    }
}

impl PartialOrd for VarValue {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for VarValue {
    fn eq(&self, other: &Self) -> bool {
        self.as_str() == other.as_str()
    }
}

impl PartialEq<Arc<str>> for VarValue {
    fn eq(&self, other: &Arc<str>) -> bool {
        self.as_str() == &**other
    }
}

impl PartialEq<VarValue> for Arc<str> {
    fn eq(&self, other: &VarValue) -> bool {
        other.as_str() == &**self
    }
}

impl std::fmt::Display for VarValue {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.as_str().fmt(f)
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) enum RequestVS {
    SpkRequest(Request),
    GlobalVar { key: String, value: VarValue },
}

impl std::fmt::Display for RequestVS {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            RequestVS::SpkRequest(req) => write!(f, "{req}"),
            RequestVS::GlobalVar { key, value } => write!(f, "GlobalVar({key}={value})"),
        }
    }
}

/// The component portion of a package that can exist in a solution.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) enum SyntheticComponent {
    /// This represents the "parent" of any components of a package, used to
    /// prevent components from different versions of a package from getting
    /// into the same solution.
    Base,
    /// Real components.
    Actual(Component),
}

impl SyntheticComponent {
    #[inline]
    pub(crate) fn is_all(&self) -> bool {
        matches!(self, SyntheticComponent::Actual(Component::All))
    }
}

// The `requires_build_from_source` field is ignored for hashing and equality
// purposes.
#[derive(Clone, Debug)]
pub(crate) struct LocatedBuildIdentWithComponent {
    pub(crate) ident: LocatedBuildIdent,
    pub(crate) component: SyntheticComponent,
    pub(crate) requires_build_from_source: bool,
}

impl std::hash::Hash for LocatedBuildIdentWithComponent {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.ident.hash(state);
        self.component.hash(state);
    }
}

impl Eq for LocatedBuildIdentWithComponent {}

impl PartialEq for LocatedBuildIdentWithComponent {
    fn eq(&self, other: &Self) -> bool {
        self.ident == other.ident && self.component == other.component
    }
}

impl LocatedBuildIdentWithComponent {
    /// Create a request that will match this ident but with a different
    /// component name.
    pub(crate) fn as_request_with_components(
        &self,
        components: impl IntoIterator<Item = Component>,
    ) -> Request {
        let mut range_ident = RangeIdent::double_equals(&self.ident.to_any_ident(), components);
        range_ident.repository_name = Some(self.ident.repository_name().to_owned());

        let mut pkg_request = PkgRequest::new(
            range_ident,
            RequestedBy::BinaryBuild(self.ident.target().clone()),
        );
        // Since we're using double_equals, is it safe to always enable
        // prereleases? If self represents a prerelease, then the Request
        // needs to allow it.
        pkg_request.prerelease_policy = Some(PreReleasePolicy::IncludeAll);

        Request::Pkg(pkg_request)
    }
}

impl PartialEq<SyntheticComponent> for Component {
    fn eq(&self, other: &SyntheticComponent) -> bool {
        match other {
            SyntheticComponent::Base => false,
            SyntheticComponent::Actual(other) => self == other,
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) enum SpkSolvable {
    LocatedBuildIdentWithComponent(LocatedBuildIdentWithComponent),
    GlobalVar { key: String, value: VarValue },
}

impl std::fmt::Display for SpkSolvable {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            SpkSolvable::LocatedBuildIdentWithComponent(located_build_ident_with_component) => {
                write!(f, "{located_build_ident_with_component}")
            }
            SpkSolvable::GlobalVar { key, value } => write!(f, "GlobalVar({key}={value})"),
        }
    }
}

impl VersionSet for RequestVS {
    type V = SpkSolvable;
}

impl std::fmt::Display for LocatedBuildIdentWithComponent {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match &self.component {
            SyntheticComponent::Base => self.ident.fmt(f),
            SyntheticComponent::Actual(component) => {
                write!(f, "{}:{component}", self.ident)
            }
        }
    }
}
