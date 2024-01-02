// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::collections::{HashMap, HashSet};

use itertools::Itertools;
use serde::{Deserialize, Serialize};
use spk_schema_foundation::option_map::Stringified;
use strum::Display;

use super::{v0, Opt, ValidationSpec};
use crate::name::{OptName, OptNameBuf};
use crate::option::VarOpt;
use crate::{Result, Variant};

#[cfg(test)]
#[path = "./build_spec_test.rs"]
mod build_spec_test;

// Each HostCompat value adds a different set of host related options
// when used.
const DISTRO_ADDS: &[&OptName] = &[OptName::os(), OptName::arch(), OptName::distro()];
const ARCH_ADDS: &[&OptName] = &[OptName::os(), OptName::arch()];
const OS_ADDS: &[&OptName] = &[OptName::os()];
const NONE_ADDS: &[&OptName] = &[];

// Each HostCompat value disallows certain var names when host_compat
// validation is enabled in the config file.
const DISTRO_DISALLOWS: &[&OptName] = &[];
const ARCH_DISALLOWS: &[&OptName] = &[OptName::distro()];
const OS_DISALLOWS: &[&OptName] = &[OptName::distro(), OptName::arch()];
const NONE_DISALLOWS: &[&OptName] = &[OptName::distro(), OptName::arch(), OptName::os()];

/// Describes what level of cross-platform compatibility the built package
/// should have.
#[derive(
    Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize, Display, Default,
)]
pub enum HostCompat {
    /// Package can only be used on the same OS distribution. Adds
    /// distro, arch, os, and <distroname> option vars.
    #[default]
    Distro,
    /// Package can be used anywhere that has the same OS and cpu
    /// type. Adds distro, and arch options vars.
    Arch,
    /// Package can be used on the same OS with any cpu or distro. Adds os option var.
    Os,
    /// Package can be used on any Os. Does not add any option vars.
    None,
}

impl HostCompat {
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }

    fn names_added(&self) -> HashSet<&OptName> {
        let names = match self {
            HostCompat::Distro => DISTRO_ADDS,
            HostCompat::Arch => ARCH_ADDS,
            HostCompat::Os => OS_ADDS,
            HostCompat::None => NONE_ADDS,
        };

        names.iter().copied().collect::<HashSet<&OptName>>()
    }

    // TODO: should this be a hashset too?
    fn names_disallowed(&self) -> &[&OptName] {
        match self {
            HostCompat::Distro => DISTRO_DISALLOWS,
            HostCompat::Arch => ARCH_DISALLOWS,
            HostCompat::Os => OS_DISALLOWS,
            HostCompat::None => NONE_DISALLOWS,
        }
    }

    /// Get host_options after filtering based on the cross Os
    /// compatibility setting.
    pub fn host_options(&self) -> Result<Vec<Opt>> {
        let all_host_options = spk_schema_foundation::option_map::host_options()?;

        let mut names_added = self.names_added();
        let distro_name;
        let fallback_name: OptNameBuf;
        if HostCompat::Distro == *self {
            match all_host_options.get(OptName::distro()) {
                Some(distro) => {
                    distro_name = distro.clone();
                    match OptName::new(&distro_name) {
                        Ok(name) => _ = names_added.insert(name),
                        Err(err) => {
                            fallback_name = OptNameBuf::new_lossy(&distro_name);
                            tracing::warn!("Reported distro id ({}) is not a valid var option name: {err}. A {} var will be used instead.",
                                           distro_name.to_string(),
                                           fallback_name);

                            _ = names_added.insert(&fallback_name);
                        }
                    }
                }
                None => {
                    tracing::warn!(
                        "No distro name set by host. A {}= will be used instead.",
                        OptName::unknown_distro()
                    );
                    _ = names_added.insert(OptName::unknown_distro());
                }
            }
        }

        let mut settings = Vec::new();
        for (name, _value) in all_host_options.iter() {
            if names_added.contains(&OptName::new(name)?) {
                let opt = Opt::Var(VarOpt::new(name)?);
                settings.push(opt)
            }
        }

        Ok(settings)
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
    pub host_compat: HostCompat,
}

impl Default for BuildSpec {
    fn default() -> Self {
        Self {
            script: Script(vec!["sh ./build.sh".into()]),
            options: Vec::new(),
            variants: vec![v0::Variant::default()],
            validation: ValidationSpec::default(),
            host_compat: HostCompat::default(),
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
        variants.first() == Some(&v0::Variant::default())
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
        let mut known_pkg_options_with_index = opts
            .iter()
            .enumerate()
            .filter_map(|(i, o)| match o {
                Opt::Pkg(_) => Some((o.full_name().to_owned(), i)),
                _ => None,
            })
            .collect::<HashMap<_, _>>();

        // inject additional package options for items in the variant that
        // were not present in the original package
        let reqs = variant.additional_requirements().into_owned();
        for req in reqs.into_iter() {
            let mut opt = Opt::try_from(req)?;

            if known.insert(opt.full_name().to_owned()) {
                // Maintain pkg index when inserting a new PkgOpt.
                if let Opt::Pkg(_) = &opt {
                    known_pkg_options_with_index.insert(opt.full_name().to_owned(), opts.len());
                };

                opts.push(opt);
                continue;
            }

            if let Opt::Pkg(pkg) = &mut opt {
                // This is an existing PkgOpt; merge the requests.

                match known_pkg_options_with_index.get(pkg.pkg.as_opt_name()) {
                    Some(&idx) => {
                        match &mut opts[idx] {
                            Opt::Pkg(pkg_in_opts) => {
                                // Merge the components of the existing option with the
                                // additional one(s) from the variant.
                                let pkg_components = std::mem::take(&mut pkg.components);
                                pkg_in_opts.components.extend(pkg_components.into_inner());

                                // The default value is overridden by the
                                // variant.
                                pkg_in_opts.default = std::mem::take(&mut pkg.default);
                            }
                            Opt::Var(_) => {
                                debug_assert!(
                                    false,
                                    "known_pkg_options_with_index should only index PkgOpt options"
                                );
                            }
                        };
                    }
                    None => {
                        debug_assert!(
                            false,
                            "known_pkg_options_with_index should already contain all PkgOpt names"
                        );
                    }
                };
            }
        }

        // Add any host options that are not already present.
        let host_opts = self.host_compat.host_options()?;
        for opt in host_opts.iter() {
            if known.insert(opt.full_name().to_owned()) {
                opts.push(opt.clone());
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
                let mut variants = Vec::<v0::VariantSpec>::new();
                let mut unchecked = BuildSpec::default();
                let has_auto_host_vars_field = false;

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
                        "auto_host_vars" => {
                            unchecked.host_compat = map.next_value::<HostCompat>()?;
                            has_auto_host_vars_field = true;
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
                    .map(|o| v0::Variant::from_spec(o, &unchecked.options))
                    .collect::<Result<Vec<_>>>()
                    .map_err(serde::de::Error::custom)?;

                if !has_auto_host_vars_field {
                    // TODO: might want to make a MissingKey or something lint
                    // instead of reusing the one in the linting PR as a placeholder
                    //let lint = UnknownKey::new("host_compat", vec!["host_compat"]);
                    self.lints.push(format!(
                        "Missing build spec field: host_compat\nWill default to 'host_compat: {}'",
                        HostCompat::default().to_string()
                    ));
                }

                // Check if any variants set a value on a var that the
                // host_compat value would normally set, or one it
                // would normally disallow.
                let added_names = unchecked.host_compat.names_added();
                let disallowed_names = unchecked.host_compat.names_disallowed();

                // TODO: to variant numbers start at 1 or 0?
                let number = 1;
                for variant_opts in variants {
                    for add_name in added_names {
                        // Caompre btreemp vs hashset, and ones a vec, make it a hashset too?
                        if let Some(value) = variant_opts.get(&*add_name) {
                            // This variant will override a host
                            // option added by the host_compat.
                            self.lints.push(format!("Variant {number} sets {add_name}={value} which overrides the {} host_compat's setting for {add_name}", unchecked.host_compat));
                        }
                    }
                    for bad_name in disallowed_names {
                        if let Some(value) = variant_opts.get(*bad_name) {
                            // This variant will set a host option
                            // the host_compat disallows.
                            self.lints.push(format!("Variant {number} sets {bad_name}={value} but the 'host_compat' value of {} disallows setting {bad_name}", unchecked.host_compat));
                        }
                    }
                    number += 1;
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
