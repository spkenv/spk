// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use pyo3::prelude::*;
use serde::{Deserialize, Serialize};

use crate::Result;

#[cfg(test)]
#[path = "./component_spec_test.rs"]
mod component_spec_test;

/// Defines a named package component.
#[pyclass]
#[derive(Debug, Default, Hash, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComponentSpec {
    #[pyo3(get)]
    #[serde(deserialize_with = "deserialize_component_name")]
    name: String,
    #[pyo3(get, set)]
    #[serde(default)]
    pub requirements: super::RequirementsList,
    #[pyo3(get, set)]
    #[serde(default)]
    pub embedded: super::EmbeddedPackagesList,
}

impl ComponentSpec {
    pub fn new<S: Into<String>>(name: S) -> Result<Self> {
        let name = name.into();
        super::validate_name(&name)?;
        Ok(Self {
            name,
            requirements: Default::default(),
            embedded: Default::default(),
        })
    }

    /// The name of this component.
    pub fn name(&self) -> &str {
        &self.name
    }
}

fn deserialize_component_name<'de, D>(deserializer: D) -> std::result::Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    super::validate_name(&value).map_err(|err| {
        serde::de::Error::invalid_value(
            serde::de::Unexpected::Str(&value),
            &err.to_string().as_str(),
        )
    })?;
    Ok(value)
}
