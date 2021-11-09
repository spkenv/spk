// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use pyo3::prelude::*;
use serde::{Deserialize, Serialize};

use super::{ComponentSpec, EmbeddedPackagesList, Ident, OptionMap, Request, RequirementsList};
use crate::Result;

#[cfg(test)]
#[path = "./install_spec_test.rs"]
mod install_spec_test;

/// A set of structured installation parameters for a package.
#[pyclass]
#[derive(Debug, Default, Hash, Clone, PartialEq, Eq, Serialize)]
pub struct InstallSpec {
    #[pyo3(get, set)]
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requirements: RequirementsList,
    #[pyo3(get, set)]
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub embedded: EmbeddedPackagesList,
    #[pyo3(get, set)]
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub components: Vec<ComponentSpec>,
}

#[pymethods]
impl InstallSpec {
    /// Add or update a requirement to the set of installation requirements.
    ///
    /// If a request exists for the same name, it is replaced with the given
    /// one. Otherwise the new request is appended to the list.
    pub fn upsert_requirement(&mut self, request: Request) {
        self.requirements.upsert(request);
    }
}

impl InstallSpec {
    pub fn is_empty(&self) -> bool {
        self.requirements.is_empty() && self.embedded.is_empty()
    }

    /// Render all requests with a package pin using the given resolved packages.
    pub fn render_all_pins<'a>(
        &mut self,
        options: &OptionMap,
        resolved: impl Iterator<Item = &'a Ident>,
    ) -> Result<()> {
        self.requirements.render_all_pins(options, resolved)
    }
}

impl<'de> Deserialize<'de> for InstallSpec {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Unchecked {
            #[serde(default)]
            requirements: RequirementsList,
            #[serde(default)]
            embedded: EmbeddedPackagesList,
            #[serde(default)]
            components: Vec<ComponentSpec>,
        }

        let unchecked = Unchecked::deserialize(deserializer)?;
        let spec = InstallSpec {
            requirements: unchecked.requirements,
            embedded: unchecked.embedded,
            components: unchecked.components,
        };

        let mut components = std::collections::HashSet::new();
        for component in spec.components.iter() {
            if !components.insert(component.name()) {
                return Err(serde::de::Error::custom(format!(
                    "found multiple compoenents with the name '{}'",
                    component.name()
                )));
            }
        }
        for component in spec.components.iter() {
            for name in component.uses.iter() {
                if !components.contains(&name.as_str()) {
                    return Err(serde::de::Error::custom(format!(
                        "component '{}' uses '{}', but it does not exist",
                        component.name(),
                        name
                    )));
                }
            }
        }

        Ok(spec)
    }
}
