// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::collections::HashSet;

use itertools::Itertools;
use serde::{Deserialize, Serialize};
use spk_schema_foundation::option_map::Stringified;

use super::foundation::option_map::OptionMap;
use super::{Opt, ValidationSpec};

#[cfg(test)]
#[path = "./build_spec_test.rs"]
mod build_spec_test;

/// A set of structured inputs used to build a package.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct BuildSpec {
    pub script: Script,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<Opt>,
    #[serde(default, skip_serializing_if = "BuildSpec::is_default_variants")]
    pub variants: Option<Vec<OptionMap>>,
    #[serde(default, skip_serializing_if = "ValidationSpec::is_default")]
    pub validation: ValidationSpec,
}

impl Default for BuildSpec {
    fn default() -> Self {
        Self {
            script: Script(vec!["sh ./build.sh".into()]),
            options: Vec::new(),
            variants: None,
            validation: ValidationSpec::default(),
        }
    }
}

impl BuildSpec {
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }

    fn is_default_variants(variants: &Option<Vec<OptionMap>>) -> bool {
        match variants {
            None => true,
            Some(v) if v.is_empty() => true,
            Some(v) if v.len() > 1 => false,
            Some(v) => v[0] == OptionMap::default(),
        }
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

impl TryFrom<UncheckedBuildSpec> for BuildSpec {
    type Error = crate::Error;

    fn try_from(bs: UncheckedBuildSpec) -> Result<Self, Self::Error> {
        let bs = unsafe {
            // Safety: this function bypasses checks, but we are
            // going to perform those checks before returning the value
            bs.into_inner()
        };

        if let Some(variants) = bs.variants.as_ref() {
            let mut variant_builds = Vec::new();
            let mut unique_variants = HashSet::new();
            for variant in variants.iter() {
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
                return Err(crate::Error::String(format!(
                    "Multiple variants would produce the same build:\n{}",
                    details
                )));
            }
        }

        Ok(bs)
    }
}

impl<'de> Deserialize<'de> for BuildSpec {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        UncheckedBuildSpec::deserialize(deserializer)
            .and_then(|bs| bs.try_into().map_err(serde::de::Error::custom))
    }
}

/// A [`BuildSpec`] that can be deserialized more forgivingly.
///
/// This exists to help with backwards-compatibility where the data
/// being deserialized must be trusted (eg it's from a repository)
/// but may also not adhere to all of the (potentially new) validation
/// that is done on the normal build spec
pub(crate) struct UncheckedBuildSpec(BuildSpec);

impl UncheckedBuildSpec {
    /// Unwrap this instance into a true validated [`BuildSpec`].
    ///
    /// This function is unsafe, [`TryInto::try_into`] can
    /// be used instead to perform the necessary validations.
    ///
    /// # Safety:
    /// This function bypassed additional
    /// validation of the internal build spec data
    /// which should usually be done
    pub unsafe fn into_inner(self) -> BuildSpec {
        self.0
    }
}

impl<'de> Deserialize<'de> for UncheckedBuildSpec {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct UncheckedBuildSpecVisitor;

        impl<'de> serde::de::Visitor<'de> for UncheckedBuildSpecVisitor {
            type Value = UncheckedBuildSpec;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a build specification")
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                let mut unchecked = BuildSpec::default();
                while let Some(key) = map.next_key::<Stringified>()? {
                    match key.as_str() {
                        "script" => unchecked.script = map.next_value::<Script>()?,
                        "options" => {
                            unchecked.options = map.next_value::<Vec<Opt>>()?;
                            let mut unique_options = HashSet::new();
                            for opt in unchecked.options.iter() {
                                let full_name = opt.full_name();
                                if unique_options.contains(full_name) {
                                    return Err(serde::de::Error::custom(format!(
                                        "build option was specified more than once: {full_name}",
                                    )));
                                }
                                unique_options.insert(full_name);
                            }
                        }
                        "variants" => {
                            unchecked.variants = Some(map.next_value::<Vec<OptionMap>>()?)
                        }
                        "validation" => {
                            unchecked.validation = map.next_value::<ValidationSpec>()?
                        }
                        _ => {
                            // for forwards compatibility we ignore any unrecognized
                            // field, but consume it just the same
                            // TODO: could we check for possible typos in here?
                            map.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }

                Ok(UncheckedBuildSpec(unchecked))
            }
        }

        deserializer.deserialize_map(UncheckedBuildSpecVisitor)
    }
}

/// Some shell script to be executed
#[derive(Hash, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Script(Vec<String>);

impl std::ops::Deref for Script {
    type Target = Vec<String>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Script {
    /// Create a new instance that contains the given lines of script.
    pub fn new<I, S>(script: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self(script.into_iter().map(Into::into).collect())
    }
}

impl From<Vec<String>> for Script {
    fn from(v: Vec<String>) -> Self {
        Self(v)
    }
}

impl<'de> Deserialize<'de> for Script {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct ScriptVisitor;

        impl<'de> serde::de::Visitor<'de> for ScriptVisitor {
            type Value = Vec<String>;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a string or list of strings")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let mut script = Vec::with_capacity(seq.size_hint().unwrap_or(0));
                while let Some(line) = seq.next_element::<Stringified>()? {
                    script.push(line.0)
                }
                Ok(script)
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(v.split('\n').map(String::from).collect())
            }
        }
        deserializer.deserialize_any(ScriptVisitor).map(Self)
    }
}

impl serde::ser::Serialize for Script {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.collect_seq(self.0.iter())
    }
}
