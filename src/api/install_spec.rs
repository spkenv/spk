// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use pyo3::prelude::*;
use serde::{Deserialize, Serialize};

use super::{ComponentSpecList, EmbeddedPackagesList, Ident, OptionMap, Request, RequirementsList};
use crate::Result;

#[cfg(test)]
#[path = "./install_spec_test.rs"]
mod install_spec_test;

/// A set of structured installation parameters for a package.
#[pyclass]
#[derive(Debug, Hash, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallSpec {
    #[pyo3(get, set)]
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requirements: RequirementsList,
    #[pyo3(get, set)]
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub embedded: EmbeddedPackagesList,
    #[pyo3(get, set)]
    #[serde(default)]
    pub components: ComponentSpecList,
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
    pub fn is_default(&self) -> bool {
        self.requirements.is_empty() && self.embedded.is_empty() && self.components.is_default()
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
