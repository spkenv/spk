// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use resolvo::utils::VersionSet;
use spk_schema::Request;
use spk_schema::ident::LocatedBuildIdent;
use spk_schema::ident_component::Component;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
#[repr(transparent)]
pub(crate) struct RequestVS(pub(crate) Request);

/// Like `Component` but without the `All` variant.
#[derive(Clone, Eq, Hash, PartialEq)]
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

#[derive(Clone, Eq, Hash, PartialEq)]
pub(crate) struct LocatedBuildIdentWithComponent {
    pub(crate) ident: LocatedBuildIdent,
    pub(crate) component: ComponentWithoutAll,
}

impl VersionSet for RequestVS {
    type V = LocatedBuildIdentWithComponent;
}

impl std::fmt::Display for LocatedBuildIdentWithComponent {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}:{}", self.ident, self.component)
    }
}
