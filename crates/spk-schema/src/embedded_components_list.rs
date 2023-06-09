// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::BTreeSet;
use std::fmt::Write;

use serde::{Deserialize, Serialize};
use spk_schema_foundation::ident_component::{Component, Components};
use spk_schema_foundation::ident_ops::parsing::request_pkg_name_and_version;
use spk_schema_ident::OptVersionIdent;

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EmbeddedComponents {
    pub pkg: OptVersionIdent,
    pub components: BTreeSet<Component>,
}

impl std::fmt::Display for EmbeddedComponents {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.pkg.name().fmt(f)?;
        self.components.fmt_component_set(f)?;
        if let Some(version) = self.pkg.target() {
            f.write_char('/')?;
            version.fmt(f)?;
        }
        Ok(())
    }
}

impl<'de> Deserialize<'de> for EmbeddedComponents {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct EmbeddedComponentsVisitor;

        impl<'de> serde::de::Visitor<'de> for EmbeddedComponentsVisitor {
            type Value = EmbeddedComponents;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("an embedded components")
            }

            fn visit_str<E>(self, v: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                request_pkg_name_and_version::<nom_supreme::error::ErrorTree<_>>(v)
                    .map(|(_, (name, components, opt_version))| Self::Value {
                        pkg: OptVersionIdent::new(name.to_owned(), opt_version),
                        components,
                    })
                    .map_err(|err| match err {
                        nom::Err::Error(e) | nom::Err::Failure(e) => {
                            serde::de::Error::custom(e.to_string())
                        }
                        nom::Err::Incomplete(_) => unreachable!(),
                    })
            }
        }

        deserializer.deserialize_str(EmbeddedComponentsVisitor)
    }
}

impl Serialize for EmbeddedComponents {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&format!("{self}"))
    }
}

/// A set of packages that are embedded/provided by another.
#[derive(Clone, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct EmbeddedComponentsList {
    components: Vec<EmbeddedComponents>,
    #[serde(skip)]
    fabricated: bool,
}

impl EmbeddedComponentsList {
    /// Return if the list was fabricated by defaults.
    #[inline]
    pub fn is_fabricated(&self) -> bool {
        self.fabricated
    }

    /// Mark this list as having been fabricated by defaults.
    #[inline]
    pub fn set_fabricated(&mut self) {
        self.fabricated = true;
    }
}

impl std::ops::Deref for EmbeddedComponentsList {
    type Target = Vec<EmbeddedComponents>;
    fn deref(&self) -> &Self::Target {
        &self.components
    }
}

impl std::ops::DerefMut for EmbeddedComponentsList {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.components
    }
}

impl<I> From<I> for EmbeddedComponentsList
where
    I: IntoIterator<Item = EmbeddedComponents>,
{
    fn from(items: I) -> Self {
        Self {
            components: items.into_iter().collect(),
            fabricated: false,
        }
    }
}
