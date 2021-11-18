// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use pyo3::prelude::*;
use serde::{Deserialize, Serialize};

use super::{Component, ComponentSpec};

#[cfg(test)]
#[path = "./component_spec_list_test.rs"]
mod component_spec_list_test;

/// A set of packages that are embedded/provided by another.
#[derive(Debug, Hash, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct ComponentSpecList(Vec<ComponentSpec>);

impl ComponentSpecList {
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }
}

impl Default for ComponentSpecList {
    fn default() -> Self {
        Self(vec![
            ComponentSpec::default_build(),
            ComponentSpec::default_run(),
        ])
    }
}

impl std::ops::Deref for ComponentSpecList {
    type Target = Vec<ComponentSpec>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for ComponentSpecList {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<'de> Deserialize<'de> for ComponentSpecList {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let mut unchecked = Vec::<ComponentSpec>::deserialize(deserializer)?;

        let mut components = std::collections::HashSet::new();
        for component in unchecked.iter() {
            if !components.insert(&component.name) {
                return Err(serde::de::Error::custom(format!(
                    "found multiple components with the name '{}'",
                    component.name
                )));
            }
        }

        for component in unchecked.iter() {
            for name in component.uses.iter() {
                if !components.contains(&name) {
                    return Err(serde::de::Error::custom(format!(
                        "component '{}' uses '{}', but it does not exist",
                        component.name, name
                    )));
                }
            }
        }

        let mut additional = Vec::new();
        if !components.contains(&Component::Build) {
            additional.push(ComponentSpec::default_build());
        }
        if !components.contains(&Component::Run) {
            additional.push(ComponentSpec::default_run());
        }
        if components.contains(&Component::All) {
            return Err(serde::de::Error::custom(
                "The 'all' component is reserved, and cannot be defined in a spec".to_string(),
            ));
        }
        unchecked.append(&mut additional);

        unchecked.sort_by(|a, b| a.name.cmp(&b.name));

        Ok(ComponentSpecList(unchecked))
    }
}

impl IntoPy<Py<pyo3::types::PyAny>> for ComponentSpecList {
    fn into_py(self, py: Python) -> Py<pyo3::types::PyAny> {
        self.0.into_py(py)
    }
}

impl<'source> FromPyObject<'source> for ComponentSpecList {
    fn extract(ob: &'source PyAny) -> PyResult<Self> {
        Ok(ComponentSpecList(Vec::<ComponentSpec>::extract(ob)?))
    }
}
