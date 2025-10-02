// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use spk_schema_foundation::option_map::OptionMap;

use crate::{Result, SpecFileData};

/// A recipe template for building multiple versions of a package.
#[derive(Debug, Clone, Hash, PartialEq, Eq, Ord, PartialOrd, Deserialize, Serialize)]
pub struct TemplateSpec {
    pub versions: VersionDiscovery,
}

/// Defines how to discover the versions a template supports.
#[derive(Debug, Clone, Hash, PartialEq, Eq, Ord, PartialOrd, Deserialize, Serialize)]
pub struct VersionDiscovery {
    pub discover: DiscoveryStrategy,
}

/// The strategy for discovering versions.
#[derive(Debug, Clone, Hash, PartialEq, Eq, Ord, PartialOrd, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryStrategy {
    pub git_tags: GitTagsDiscovery,
}

/// Configuration for discovering versions from git tags.
#[derive(Debug, Clone, Hash, PartialEq, Eq, Ord, PartialOrd, Deserialize, Serialize)]
pub struct GitTagsDiscovery {
    pub url: String,
    #[serde(rename = "match")]
    pub match_pattern: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extract: Option<String>,
}

/// Can be rendered into a recipe.
#[enum_dispatch::enum_dispatch]
pub trait Template: Sized {
    /// Identify the location of this template on disk
    fn file_path(&self) -> &Path;

    /// Render this template with the provided values.
    fn render(&self, options: &OptionMap) -> Result<SpecFileData>;
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
