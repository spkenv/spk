// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::sync::Arc;

use resolvo::utils::VersionSet;
use spk_schema::Request;
use spk_schema::ident::LocatedBuildIdent;
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

/// Like `Component` but without the `All` variant.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) enum ComponentWithoutAll {
    Build,
    Run,
    Source,
    Named(String),
}

impl From<ComponentWithoutAll> for Component {
    fn from(c: ComponentWithoutAll) -> Self {
        match c {
            ComponentWithoutAll::Build => Component::Build,
            ComponentWithoutAll::Run => Component::Run,
            ComponentWithoutAll::Source => Component::Source,
            ComponentWithoutAll::Named(name) => Component::Named(name),
        }
    }
}

impl TryFrom<Component> for ComponentWithoutAll {
    type Error = Component;

    fn try_from(c: Component) -> Result<Self, Self::Error> {
        match c {
            Component::All => Err(Component::All),
            Component::Build => Ok(ComponentWithoutAll::Build),
            Component::Run => Ok(ComponentWithoutAll::Run),
            Component::Source => Ok(ComponentWithoutAll::Source),
            Component::Named(name) => Ok(ComponentWithoutAll::Named(name)),
        }
    }
}

impl std::fmt::Display for ComponentWithoutAll {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            ComponentWithoutAll::Build => write!(f, "build"),
            ComponentWithoutAll::Run => write!(f, "run"),
            ComponentWithoutAll::Source => write!(f, "source"),
            ComponentWithoutAll::Named(name) => write!(f, "{name}"),
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct LocatedBuildIdentWithComponent {
    pub(crate) ident: LocatedBuildIdent,
    pub(crate) component: ComponentWithoutAll,
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
        write!(f, "{}:{}", self.ident, self.component)
    }
}
