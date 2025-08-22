// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::BTreeSet;
use std::fmt::Write;

use serde::{Deserialize, Serialize};
use spk_schema_foundation::ident::OptVersionIdent;
use spk_schema_foundation::ident_component::{Component, Components};
use spk_schema_foundation::ident_ops::parsing::request_pkg_name_and_version;

use crate::{Error, Result};

/// A struct describing a package that is embedded within a component of a
/// host package.
///
/// A component of a host package may embed another package but only contain
/// some of the files of the embedded package. This struct describes the
/// identity of the embedded package and the components that are embedded.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ComponentEmbeddedPackage {
    pub pkg: OptVersionIdent,
    /// The components of the embedded package that are present in this
    /// component. Must not be empty.
    components: BTreeSet<Component>,
}

impl ComponentEmbeddedPackage {
    /// Create a new `ComponentEmbeddedPackage` with a single component.
    pub fn new(pkg: OptVersionIdent, component: Component) -> Self {
        Self {
            pkg,
            components: [component].into(),
        }
    }

    #[inline]
    pub fn components(&self) -> &BTreeSet<Component> {
        &self.components
    }

    /// Extend the set of components by the given iterable, replacing the `All`
    /// component if present. The resulting set must not be empty.
    pub fn replace_all<Iter>(&mut self, iter: Iter) -> Result<()>
    where
        Iter: IntoIterator<Item = Component>,
    {
        let mut components = BTreeSet::new();
        components.extend(self.components.iter().filter(|c| !c.is_all()).cloned());
        for component in iter {
            if component.is_all() {
                return Err(Error::String(
                    "the All component is not allowed when replacing All".to_string(),
                ));
            }
            components.insert(component);
        }
        if components.is_empty() {
            return Err(Error::String(
                "the resulting component set would be empty".to_string(),
            ));
        }
        self.components = components;
        Ok(())
    }
}

impl std::fmt::Display for ComponentEmbeddedPackage {
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

impl<'de> Deserialize<'de> for ComponentEmbeddedPackage {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct EmbeddedComponentsVisitor;

        impl serde::de::Visitor<'_> for EmbeddedComponentsVisitor {
            type Value = ComponentEmbeddedPackage;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("an embedded components")
            }

            fn visit_str<E>(self, v: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                request_pkg_name_and_version::<nom_supreme::error::ErrorTree<_>>(v)
                    .map_err(|err| match err {
                        nom::Err::Error(e) | nom::Err::Failure(e) => {
                            serde::de::Error::custom(e.to_string())
                        }
                        nom::Err::Incomplete(_) => unreachable!(),
                    })
                    .and_then(|(_, (name, components, opt_version))| {
                        if components.is_empty() {
                            return Err(serde::de::Error::custom(
                                "the components embedded in this component must not be empty, use :all to embed all components",
                            ));
                        }

                        Ok(Self::Value {
                            pkg: OptVersionIdent::new(name.to_owned(), opt_version),
                            components,
                        })
                    })
            }
        }

        deserializer.deserialize_str(EmbeddedComponentsVisitor)
    }
}

impl Serialize for ComponentEmbeddedPackage {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&format!("{self}"))
    }
}

/// A set of packages that are embedded within a component.
#[derive(Clone, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct ComponentEmbeddedPackagesList {
    components: Vec<ComponentEmbeddedPackage>,
    #[serde(skip)]
    fabricated: bool,
}

impl ComponentEmbeddedPackagesList {
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

impl std::ops::Deref for ComponentEmbeddedPackagesList {
    type Target = Vec<ComponentEmbeddedPackage>;
    fn deref(&self) -> &Self::Target {
        &self.components
    }
}

impl std::ops::DerefMut for ComponentEmbeddedPackagesList {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.components
    }
}

impl<I> From<I> for ComponentEmbeddedPackagesList
where
    I: IntoIterator<Item = ComponentEmbeddedPackage>,
{
    fn from(items: I) -> Self {
        Self {
            components: items.into_iter().collect(),
            fabricated: false,
        }
    }
}
