// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::{
    collections::{BTreeSet, HashSet},
    convert::TryFrom,
    fmt::{Display, Write},
};

use colored::Colorize;
use itertools::Itertools;
use serde::{Deserialize, Serialize};

use crate::format::FormatComponents;
use crate::name::PkgName;

use super::{Error, Result};

#[cfg(test)]
#[path = "./component_spec_test.rs"]
mod component_spec_test;

pub trait Components {
    /// Render a set of [`Component`].
    ///
    /// An empty set is an empty string.
    /// A set with a single entry is formatted as `":name"`.
    /// A set with multiple entries is formatted as `":{name1,name2}"`.
    fn fmt_component_set(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result;
}

/// Identifies a component by name
#[derive(Debug, Hash, Clone, Eq, PartialEq, PartialOrd, Ord)]
pub enum Component {
    All,
    Build,
    Run,
    Source,
    Named(String),
}

impl Component {
    /// Return the default build component based on migration-to-components feature
    #[inline]
    pub fn default_for_build() -> Self {
        // For sites that started using spk after component support was added
        #[cfg(not(feature = "migration-to-components"))]
        return Component::Build;

        // For migrating to packages with components while a site has
        // packages that were published before components were supported.
        #[cfg(feature = "migration-to-components")]
        return Component::All;
    }

    /// Return the default run component based on migration-to-components feature
    #[inline]
    pub fn default_for_run() -> Self {
        // For sites that started using spk after component support was added
        #[cfg(not(feature = "migration-to-components"))]
        return Component::Run;

        // For migrating to packages with components while a site has
        // packages that were published before components were supported.
        #[cfg(feature = "migration-to-components")]
        return Component::All;
    }

    /// Parse a component name from a string, ensuring that it's valid
    pub fn parse<S: AsRef<str>>(source: S) -> Result<Self> {
        let source = source.as_ref();
        // for now, components follow the same naming requirements as packages
        let _ = PkgName::new(source)?;
        Ok(match source {
            "all" => Self::All,
            "run" => Self::Run,
            "build" => Self::Build,
            "src" => Self::Source,
            _ => Self::Named(source.to_string()),
        })
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::All => "all",
            Self::Run => "run",
            Self::Build => "build",
            Self::Source => "src",
            Self::Named(value) => value,
        }
    }

    pub fn is_all(&self) -> bool {
        matches!(self, Self::All)
    }

    pub fn is_run(&self) -> bool {
        matches!(self, Self::Run)
    }

    pub fn is_build(&self) -> bool {
        matches!(self, Self::Build)
    }

    pub fn is_source(&self) -> bool {
        matches!(self, Self::Source)
    }

    pub fn is_named(&self) -> bool {
        matches!(self, Self::Named(_))
    }
}

impl Components for BTreeSet<Component> {
    fn fmt_component_set(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self.len() {
            0 => (),
            1 => {
                f.write_char(':')?;
                self.iter().join(",").fmt(f)?;
            }
            _ => {
                f.write_char(':')?;
                f.write_char('{')?;
                self.iter().join(",").fmt(f)?;
                f.write_char('}')?;
            }
        }
        Ok(())
    }
}

impl std::str::FromStr for Component {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self> {
        Self::parse(s)
    }
}

impl TryFrom<&str> for Component {
    type Error = Error;
    fn try_from(value: &str) -> Result<Self> {
        Self::parse(value)
    }
}

impl TryFrom<String> for Component {
    type Error = Error;
    fn try_from(value: String) -> Result<Self> {
        Self::parse(value)
    }
}

impl std::fmt::Display for Component {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_ref())
    }
}

impl AsRef<str> for Component {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl<'de> Deserialize<'de> for Component {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct ComponentVisitor;
        impl<'de> serde::de::Visitor<'de> for ComponentVisitor {
            type Value = Component;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a component name")
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Component, E>
            where
                E: serde::de::Error,
            {
                Component::parse(value).map_err(serde::de::Error::custom)
            }
        }
        deserializer.deserialize_str(ComponentVisitor)
    }
}

impl Serialize for Component {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.as_str().serialize(serializer)
    }
}

#[derive(Default)]
pub struct ComponentSet(HashSet<Component>);

impl ComponentSet {
    pub fn new() -> Self {
        Self(HashSet::new())
    }
}

impl std::ops::Deref for ComponentSet {
    type Target = HashSet<Component>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for ComponentSet {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<I> From<I> for ComponentSet
where
    I: IntoIterator<Item = Component>,
{
    fn from(iter: I) -> Self {
        Self(iter.into_iter().collect())
    }
}

impl FormatComponents for ComponentSet {
    fn format_components(&self) -> String {
        let mut components: Vec<_> = self.0.iter().map(Component::to_string).collect();
        components.sort();
        let mut out = components.join(",");
        if components.len() > 1 {
            out = format!("{}{}{}", "{".dimmed(), out, "}".dimmed(),)
        }
        out
    }
}
