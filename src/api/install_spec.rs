// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use pyo3::prelude::*;
use serde::{Deserialize, Serialize};

use super::{Build, BuildSpec, ComponentSpec, Ident, OptionMap, Request, RequirementsList, Spec};
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
    pub embedded: Vec<Spec>,
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
            embedded: Vec<Spec>,
            #[serde(default)]
            components: Vec<ComponentSpec>,
        }

        let unchecked = Unchecked::deserialize(deserializer)?;
        let mut spec = InstallSpec {
            requirements: unchecked.requirements,
            embedded: unchecked.embedded,
            components: unchecked.components,
        };

        let mut default_build_spec = BuildSpec::default();
        for embedded in spec.embedded.iter_mut() {
            default_build_spec.options = embedded.build.options.clone();
            if default_build_spec != embedded.build {
                return Err(serde::de::Error::custom(
                    "embedded packages can only specify build.options",
                ));
            }
            if !embedded.install.is_empty() {
                return Err(serde::de::Error::custom(
                    "embedded packages cannot specify the install field",
                ));
            }
            match &mut embedded.pkg.build {
                Some(Build::Embedded) => continue,
                None => embedded.pkg.set_build(Some(Build::Embedded)),
                Some(_) => {
                    return Err(serde::de::Error::custom(format!(
                        "embedded package should not specify a build, got: {}",
                        embedded.pkg
                    )));
                }
            }
        }

        Ok(spec)
    }
}
