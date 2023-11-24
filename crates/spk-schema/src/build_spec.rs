// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::collections::HashSet;

use itertools::Itertools;
use serde::{Deserialize, Serialize};
use spk_schema_foundation::option_map::Stringified;
use strum::Display;

use super::foundation::option_map::OptionMap;
use super::{v0, Opt, ValidationSpec};
use crate::name::{OptName, OptNameBuf};
use crate::option::VarOpt;
use crate::{Error, Result, Variant};

#[cfg(test)]
#[path = "./build_spec_test.rs"]
mod build_spec_test;

// TODO: could move to another file nearer the host_options() function
// in use super::foundation::option_map
/// Set what level of cross-platform compatibility the built package
/// should have.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize, Display)]
pub enum HostOsCompatibility {
    /// Package can only be used on the same OS distribution
    #[serde(alias = "distro")]
    Distro,
    /// Package can be used anywhere that has the same OS and cpu type
    #[serde(alias = "arch")]
    Arch,
    /// Package can be used on the same OS with any cpu or distro
    #[serde(alias = "os", alias = "OS")]
    Os,
    /// Package can be used on any Os
    #[serde(alias = "any")]
    Any,
}

impl Default for HostOsCompatibility {
    fn default() -> Self {
        // TODO: possibly make the default configurable for sites, or
        // remove this and derive the default?
        Self::Distro
    }
}

impl HostOsCompatibility {
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }

    /// Get host_options after filtering based on the cross Os
    /// compatibility setting.
    pub fn host_options(&self) -> Result<OptionMap> {
        let all_host_options = spk_schema_foundation::option_map::host_options()?;

        // TODO: move these to constant/config
        let names_added = match self {
            HostOsCompatibility::Distro => {
                let mut allows = vec![
                    OptName::os().to_owned(),
                    OptName::arch().to_owned(),
                    OptName::distro().to_owned(),
                ];
                // Also adds the <distroname> option
                match all_host_options.get(OptName::distro()) {
                    Some(distro_name) => match OptNameBuf::try_from(distro_name.clone()) {
                        Ok(name) => allows.push(name.to_owned()),
                        Err(_err) => {
                            return Err(Error::HostOptionNotValidDistroNameError(
                                distro_name.to_string(),
                            ))
                        }
                    },
                    None => return Err(Error::HostOptionNoDistroName),
                };
                allows
            }
            HostOsCompatibility::Arch => {
                vec![OptName::os().to_owned(), OptName::arch().to_owned()]
            }
            HostOsCompatibility::Os => {
                vec![OptName::os().to_owned()]
            }
            HostOsCompatibility::Any => Vec::new(),
        };

        let mut settings = OptionMap::default();
        for (name, value) in all_host_options.iter() {
            if names_added.contains(name) {
                settings.insert(name.to_owned(), value.clone());
            }
        }

        Ok(settings)
    }

    /// Check the given options are compatible with the
    /// HostOsCompatibility setting.
    pub fn validate_host_opts(&self, options: &[Opt]) -> Result<()> {
        let known = options
            .iter()
            .map(Opt::full_name)
            .map(ToOwned::to_owned)
            .collect::<HashSet<_>>();

        // TODO: move these to constant/config setting
        let disallowed_names = match self {
            HostOsCompatibility::Distro => Vec::new(),
            HostOsCompatibility::Arch => {
                vec![OptName::distro().to_owned()]
            }
            HostOsCompatibility::Os => {
                vec![OptName::distro().to_owned(), OptName::arch().to_owned()]
            }
            HostOsCompatibility::Any => {
                vec![
                    OptName::distro().to_owned(),
                    OptName::arch().to_owned(),
                    OptName::os().to_owned(),
                ]
            }
        };

        for name in disallowed_names {
            // If a name that this setting would add is already in the
            // given options then flag it as an error.
            if known.contains(&name) {
                return Err(Error::HostOptionNotAllowedInVariantError(
                    name.to_string(),
                    self.to_string(),
                ));
            }
        }

        Ok(())
    }
}

/// A set of structured inputs used to build a package.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct BuildSpec {
    pub script: Script,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<Opt>,
    #[serde(default, skip_serializing_if = "BuildSpec::is_default_variants")]
    pub variants: Vec<v0::Variant>,
    #[serde(default, skip_serializing_if = "ValidationSpec::is_default")]
    pub validation: ValidationSpec,
    #[serde(default)]
    pub host_compatibility: HostOsCompatibility,
}

impl Default for BuildSpec {
    fn default() -> Self {
        Self {
            script: Script(vec!["sh ./build.sh".into()]),
            options: Vec::new(),
            variants: vec![v0::Variant::default()],
            validation: ValidationSpec::default(),
            host_compatibility: HostOsCompatibility::default(),
        }
    }
}

impl BuildSpec {
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }

    fn is_default_variants(variants: &[v0::Variant]) -> bool {
        if variants.len() != 1 {
            return false;
        }
        variants.get(0) == Some(&v0::Variant::default())
    }

    /// Returns this build's options, plus any additional ones needed
    /// for building the given variant
    pub fn opts_for_variant<V>(&self, variant: &V) -> Result<Vec<Opt>>
    where
        V: Variant,
    {
        let mut opts = self.options.clone();
        let mut known = opts
            .iter()
            .map(Opt::full_name)
            .map(ToOwned::to_owned)
            .collect::<HashSet<_>>();

        // inject additional package options for items in the variant that
        // were not present in the original package
        let reqs = variant.additional_requirements().into_owned();
        for req in reqs.into_iter() {
            let opt = Opt::try_from(req)?;
            if known.insert(opt.full_name().to_owned()) {
                opts.push(opt);
            }
        }

        // Optionally, check the opts aren't setting an option
        // controlled by the host compatibility setting.
        let config = spk_config::get_config()?;
        if config.host_compat.validate {
            self.host_compatibility.validate_host_opts(&opts)?;
        }

        // Add any host options that are not already present.
        let host_opts = self.host_compatibility.host_options()?;
        for (name, value) in host_opts.iter() {
            let mut opt = Opt::Var(VarOpt::new(name)?);
            opt.set_value(value.to_string())?;
            if known.insert(opt.full_name().to_owned()) {
                opts.push(opt);
            }
        }

        Ok(opts)
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

    fn try_from(bs: UncheckedBuildSpec) -> std::result::Result<Self, Self::Error> {
        let bs = unsafe {
            // Safety: this function bypasses checks, but we are
            // going to perform those checks before returning the value
            bs.into_inner()
        };

        let mut variant_builds = Vec::new();
        let mut unique_variants = HashSet::new();
        for variant in bs.variants.iter() {
            let options = variant.options().into_owned();
            let digest = options.digest();
            variant_builds.push((digest, options));
            unique_variants.insert(digest);
        }
        if unique_variants.len() < variant_builds.len() {
            let details = variant_builds
                .iter()
                .map(|(h, o)| format!("  - {} ({})", o, h.iter().join("")))
                .collect::<Vec<_>>()
                .join("\n");
            return Err(crate::Error::String(format!(
                "Multiple variants would produce the same build:\n{details}"
            )));
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

            fn visit_map<A>(self, mut map: A) -> std::result::Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                let mut variants = Vec::<OptionMap>::new();
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
                            variants = map.next_value()?;
                        }
                        "validation" => {
                            unchecked.validation = map.next_value::<ValidationSpec>()?
                        }
                        "host_compat" | "host_compatibility" => {
                            unchecked.host_compatibility =
                                map.next_value::<HostOsCompatibility>()?
                        }
                        _ => {
                            // for forwards compatibility we ignore any unrecognized
                            // field, but consume it just the same
                            // TODO: could we check for possible typos in here?
                            map.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }

                if variants.is_empty() {
                    variants.push(Default::default());
                }

                // we can only parse out the final variant forms after all the
                // build options have been loaded
                unchecked.variants = variants
                    .into_iter()
                    .map(|o| v0::Variant::from_options(o, &unchecked.options))
                    .collect();

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

            fn visit_seq<A>(self, mut seq: A) -> std::result::Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let mut script = Vec::with_capacity(seq.size_hint().unwrap_or(0));
                while let Some(line) = seq.next_element::<Stringified>()? {
                    script.push(line.0)
                }
                Ok(script)
            }

            fn visit_str<E>(self, v: &str) -> std::result::Result<Self::Value, E>
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
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.collect_seq(self.0.iter())
    }
}
