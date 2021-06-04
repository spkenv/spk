// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::collections::HashSet;

use pyo3::prelude::*;
use serde::{Deserialize, Serialize};

use super::{Compatibility, Opt, OptionMap};

#[cfg(test)]
#[path = "./build_spec_test.rs"]
mod build_spec_test;

/// A set of structured inputs used to build a package.
#[pyclass]
#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize)]
pub struct BuildSpec {
    #[pyo3(get, set)]
    pub script: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[pyo3(get, set)]
    pub options: Vec<Opt>,
    #[serde(default, skip_serializing_if = "BuildSpec::is_default_variants")]
    #[pyo3(get, set)]
    pub variants: Vec<OptionMap>,
}

impl Default for BuildSpec {
    fn default() -> Self {
        Self {
            script: vec!["sh ./build.sh".into()],
            options: Vec::new(),
            variants: vec![OptionMap::default()],
        }
    }
}

impl BuildSpec {
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }

    fn is_default_variants(variants: &Vec<OptionMap>) -> bool {
        if variants.len() != 1 {
            return false;
        }
        variants.get(0) == Some(&OptionMap::default())
    }
}

#[pymethods]
impl BuildSpec {
    #[new]
    fn init(options: Vec<Opt>) -> Self {
        Self {
            options: options,
            ..Self::default()
        }
    }

    pub fn resolve_all_options(&self, package_name: Option<&str>, given: &OptionMap) -> OptionMap {
        let mut resolved = OptionMap::default();
        for opt in self.options.iter() {
            let name = opt.name();
            let mut given_value: Option<&String> = None;

            if let Some(name) = &package_name {
                given_value = given.get(&opt.namespaced_name(name))
            }
            if let None = &given_value {
                given_value = given.get(name)
            }

            let value = opt.get_value(&given_value.map(String::to_owned));
            resolved.insert(name.to_string(), value);
        }

        resolved
    }

    /// Validate the given options against the options in this spec.
    pub fn validate_options(&self, package_name: &str, given_options: &OptionMap) -> Compatibility {
        let mut must_exist = given_options.package_options_without_global(&package_name);
        println!("{}", given_options);
        let given_options = given_options.package_options(&package_name);
        println!("{}", given_options);
        for option in self.options.iter() {
            let value = given_options.get(option.name()).map(String::as_str);
            println!("{:?} {:?} {}", option.name(), value, given_options);
            let compat = option.validate(value);
            if !compat.is_ok() {
                return Compatibility::Incompatible(format!(
                    "invalid value for {}: {}",
                    option.name(),
                    compat
                ));
            }

            must_exist.remove(option.name());
        }

        let missing = must_exist.keys();
        if missing.len() != 0 {
            let missing = must_exist.iter().collect::<Vec<_>>();
            return Compatibility::Incompatible(format!(
                "Package does not define requested build options: {:?}",
                missing
            ));
        }

        Compatibility::Compatible
    }

    /// Add or update an option in this build spec.
    ///
    /// An option is replaced if it shares a name with the given option,
    /// otherwise the option is appended to the buid options
    pub fn upsert_opt(&mut self, opt: Opt) {
        for other in self.options.iter_mut() {
            if other.name() == opt.name() {
                let _ = std::mem::replace(other, opt);
                return;
            }
        }
        self.options.push(opt);
    }
}

impl<'de> Deserialize<'de> for BuildSpec {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Unchecked {
            #[serde(default, skip_serializing_if = "Option::is_none")]
            script: Option<Vec<String>>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            options: Option<Vec<Opt>>,
            #[serde(default, skip_serializing_if = "BuildSpec::is_default_variants")]
            variants: Vec<OptionMap>,
        }

        let raw = Unchecked::deserialize(deserializer)?;
        let mut bs = BuildSpec::default();
        if let Some(script) = raw.script {
            bs.script = script
        }
        if let Some(options) = raw.options {
            bs.options = options
        }
        if !raw.variants.is_empty() {
            bs.variants = raw.variants
        }
        let mut unique_options = HashSet::new();
        for opt in bs.options.iter() {
            let name = opt.name();
            if unique_options.contains(&name) {
                return Err(serde::de::Error::custom(format!(
                    "Build option specified more than once: {}",
                    opt.name()
                )));
            }
            unique_options.insert(name);
        }

        let mut variant_builds = Vec::new();
        let mut unique_variants = HashSet::new();
        for variant in bs.variants.iter() {
            let mut build_opts = variant.clone();
            build_opts.append(&mut bs.resolve_all_options(None, variant));
            let digest = build_opts.digest();
            variant_builds.push((digest.clone(), variant.clone()));
            unique_variants.insert(digest);
        }
        if unique_variants.len() < variant_builds.len() {
            let details = variant_builds
                .iter()
                .map(|(h, o)| format!("- {} ({:?})", o, h))
                .collect::<Vec<_>>()
                .join("\n");
            return Err(serde::de::Error::custom(format!(
                "Multiple variants would produce the same build:\n{}",
                details
            )));
        }

        Ok(bs)
    }
}
