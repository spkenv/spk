// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::HashMap;
use std::path::Path;

use crate::foundation::option_map::OptionMap;
use crate::foundation::spec_ops::Named;
use crate::Result;

/// Can be rendered into a recipe.
#[enum_dispatch::enum_dispatch]
pub trait Template: Named + Sized {
    type Output: super::Recipe;

    /// Identify the location of this template on disk
    fn file_path(&self) -> &Path;

    /// Render this template with the provided values.
    fn render(&self, options: &OptionMap) -> Result<Self::Output>;
}

pub trait TemplateExt: Template {
    /// Load this template from a file on disk
    fn from_file(path: &Path) -> Result<Self>;
}

/// The structured data that should be made available
/// when rendering spk templates into recipes
#[derive(serde::Serialize, Debug, Clone)]
pub struct TemplateData {
    /// Information about the release of spk being used
    spk: SpkInfo,
    /// The option values for this template, expanded
    /// from an option map so that namespaced options
    /// like `python.abi` actual live under the `python`
    /// field rather than as a field with a '.' in the name
    opt: serde_yaml::Mapping,
    /// Environment variable data for the current process
    env: HashMap<String, String>,
}

/// The structured data that should be made available
/// when rendering spk templates into recipes
#[derive(serde::Serialize, Debug, Clone)]
struct SpkInfo {
    version: &'static str,
}

impl Default for SpkInfo {
    fn default() -> Self {
        Self {
            version: env!("CARGO_PKG_VERSION"),
        }
    }
}

impl TemplateData {
    /// Create the set of templating data for the current process and options
    pub fn new(options: &OptionMap) -> Self {
        TemplateData {
            spk: SpkInfo::default(),
            opt: options.to_yaml_value_expanded(),
            env: std::env::vars().collect(),
        }
    }
}
