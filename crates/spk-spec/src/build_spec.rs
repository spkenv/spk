// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::collections::HashSet;

use itertools::Itertools;
use serde::{Deserialize, Serialize};
use spk_option_map::OptionMap;

use super::{Opt, ValidationSpec};

#[cfg(test)]
#[path = "./build_spec_test.rs"]
mod build_spec_test;

/// A set of structured inputs used to build a package.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct BuildSpec {
    pub script: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<Opt>,
    #[serde(default, skip_serializing_if = "BuildSpec::is_default_variants")]
    pub variants: Vec<OptionMap>,
    #[serde(default, skip_serializing_if = "ValidationSpec::is_default")]
    pub validation: ValidationSpec,
}

impl Default for BuildSpec {
    fn default() -> Self {
        Self {
            script: vec!["sh ./build.sh".into()],
            options: Vec::new(),
            variants: vec![OptionMap::default()],
            validation: ValidationSpec::default(),
        }
    }
}

impl BuildSpec {
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }

    fn is_default_variants(variants: &[OptionMap]) -> bool {
        if variants.len() != 1 {
            return false;
        }
        variants.get(0) == Some(&OptionMap::default())
    }

    /// Add or update an option in this build spec.
    ///
    /// An option is replaced if it shares a name with the given option,
    /// otherwise the option is appended to the build options
    pub fn upsert_opt(&mut self, opt: Opt) {
        for other in self.options.iter_mut() {
            if other.full_name() == opt.full_name() {
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
        let bs = BuildSpec::deserialize_unsafe(deserializer)?;

        let mut variant_builds = Vec::new();
        let mut unique_variants = HashSet::new();
        for variant in bs.variants.iter() {
            let digest = variant.digest();
            variant_builds.push((digest, variant.clone()));
            unique_variants.insert(digest);
        }
        if unique_variants.len() < variant_builds.len() {
            let details = variant_builds
                .iter()
                .map(|(h, o)| format!("  - {} ({})", o, h.iter().join("")))
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

impl<'de> BuildSpec {
    pub(crate) fn deserialize_unsafe<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Unchecked {
            #[serde(default)]
            script: Option<serde_yaml::Value>,
            #[serde(default)]
            options: Vec<Opt>,
            #[serde(default)]
            variants: Vec<OptionMap>,
            #[serde(default)]
            validation: ValidationSpec,
        }

        let raw = Unchecked::deserialize(deserializer)?;
        let mut bs = BuildSpec {
            validation: raw.validation,
            options: raw.options,
            ..BuildSpec::default()
        };
        if let Some(script) = raw.script {
            bs.script = deserialize_script(script)
                .map_err(|err| serde::de::Error::custom(format!("build.script: {}", err)))?;
        }
        if !raw.variants.is_empty() {
            bs.variants = raw.variants
        }
        let mut unique_options = HashSet::new();
        for opt in bs.options.iter() {
            let full_name = opt.full_name();
            if unique_options.contains(full_name) {
                return Err(serde::de::Error::custom(format!(
                    "Build option specified more than once: {full_name}",
                )));
            }
            unique_options.insert(full_name);
        }
        Ok(bs)
    }
}

/// Deserialize any reasonable scalar option (int, float, str) to a string value
pub(crate) fn deserialize_script<'de, D>(
    deserializer: D,
) -> std::result::Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde_yaml::Value;
    let value = Value::deserialize(deserializer)?;
    match value {
        Value::Sequence(seq) => Vec::<String>::deserialize(Value::Sequence(seq))
            .map_err(|err| serde::de::Error::custom(err.to_string())),
        Value::String(string) => Ok(string.split('\n').map(String::from).collect_vec()),
        _ => Err(serde::de::Error::custom(
            "expected string or list of strings",
        )),
    }
}
