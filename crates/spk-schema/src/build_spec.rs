// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::{HashMap, HashSet};
use std::hash::Hash;

use itertools::Itertools;
use serde::{Deserialize, Serialize};
use spk_schema_foundation::ident_build::BuildId;
use spk_schema_foundation::name::PkgName;
use spk_schema_foundation::option_map::{OptionMap, Stringified, HOST_OPTIONS};
use struct_field_names_as_array::FieldNamesAsArray;
use strum::Display;

use super::{v0, Opt, ValidationSpec};
use crate::name::{OptName, OptNameBuf};
use crate::option::{PkgOpt, VarOpt};
use crate::{Error, Lint, LintedItem, Lints, Result, UnknownKey, Variant};

#[cfg(test)]
#[path = "./build_spec_test.rs"]
mod build_spec_test;

// Each AutoHostVars value adds a different set of host related options
// when used.
const DISTRO_ADDS: &[&OptName] = &[OptName::os(), OptName::arch(), OptName::distro()];
const ARCH_ADDS: &[&OptName] = &[OptName::os(), OptName::arch()];
const OS_ADDS: &[&OptName] = &[OptName::os()];
const NONE_ADDS: &[&OptName] = &[];

/// Describes what level of cross-platform compatibility the built package
/// should have.
#[derive(
    Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize, Display, Default,
)]
pub enum AutoHostVars {
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

impl AutoHostVars {
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }

    fn names_added(&self) -> HashSet<&OptName> {
        let names = match self {
            AutoHostVars::Distro => DISTRO_ADDS,
            AutoHostVars::Arch => ARCH_ADDS,
            AutoHostVars::Os => OS_ADDS,
            AutoHostVars::None => NONE_ADDS,
        };

        names.iter().copied().collect::<HashSet<&OptName>>()
    }

    /// Get host_options after filtering based on the cross Os
    /// compatibility setting.
    pub fn host_options(&self) -> Result<Vec<Opt>> {
        let all_host_options = HOST_OPTIONS.get()?;

        let mut names_added = self.names_added();
        let distro_name;
        let fallback_name: OptNameBuf;
        if AutoHostVars::Distro == *self {
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
#[derive(Clone, Debug, Eq, FieldNamesAsArray, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct BuildSpec {
    pub script: Script,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<Opt>,
    /// The raw variant specs as they were parsed from the recipe, so the
    /// recipe can be serialized back out with the same variant spec.
    #[serde(
        default,
        rename = "variants",
        skip_serializing_if = "BuildSpec::is_default_variants"
    )]
    raw_variants: Vec<v0::VariantSpec>,
    /// The parsed variants, which are used for building.
    #[serde(skip)]
    pub variants: Vec<v0::Variant>,
    #[serde(default, skip_serializing_if = "ValidationSpec::is_default")]
    pub validation: ValidationSpec,
    #[serde(default, skip_serializing_if = "AutoHostVars::is_default")]
    pub auto_host_vars: AutoHostVars,
}

impl Default for BuildSpec {
    fn default() -> Self {
        Self {
            script: Script(vec!["sh ./build.sh".into()]),
            options: Vec::new(),
            raw_variants: vec![v0::VariantSpec::default()],
            variants: vec![v0::Variant::default()],
            validation: ValidationSpec::default(),
            auto_host_vars: AutoHostVars::default(),
        }
    }
}

impl BuildSpec {
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }

    fn is_default_variants(variants: &[v0::VariantSpec]) -> bool {
        if variants.len() != 1 {
            return false;
        }
        variants.first() == Some(&v0::VariantSpec::default())
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
        let host_opts = self.auto_host_vars.host_options()?;
        for opt in host_opts.iter() {
            if known.insert(opt.full_name().to_owned()) {
                opts.push(opt.clone());
            }
        }

        Ok(opts)
    }

    pub fn resolve_options_for_pkg_name<V>(
        &self,
        pkg_name: &PkgName,
        variant: &V,
    ) -> Result<(OptionMap, Vec<Opt>)>
    where
        V: Variant,
    {
        let given = variant.options();
        let opts = self.opts_for_variant(variant)?;
        let mut resolved = OptionMap::default();

        for opt in &opts {
            let given_value = match opt.full_name().namespace() {
                Some(_) => given
                    .get(opt.full_name())
                    .or_else(|| given.get(opt.full_name().without_namespace())),
                None => given
                    .get(&opt.full_name().with_namespace(pkg_name))
                    .or_else(|| given.get(opt.full_name())),
            };
            let value = opt.get_value(given_value.map(String::as_ref));
            let compat = opt.validate(Some(&value));
            if !compat.is_ok() {
                return Err(Error::String(compat.to_string()));
            }
            resolved.insert(opt.full_name().to_owned(), value);
        }

        Ok((resolved, opts))
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

    pub(crate) fn build_digest<V>(&self, pkg_name: &PkgName, variant: &V) -> Result<BuildId>
    where
        V: Variant,
    {
        let (options, opts) = self.resolve_options_for_pkg_name(pkg_name, variant)?;
        let mut hasher = ring::digest::Context::new(&ring::digest::SHA1_FOR_LEGACY_USE_ONLY);
        for (name, value) in options.iter() {
            hasher.update(name.as_bytes());
            hasher.update(b"=");
            hasher.update(value.as_bytes());
            hasher.update(&[0]);
        }
        for requirement in opts
            .into_iter()
            .filter_map(|opt| match opt {
                Opt::Pkg(pkg) => Some(pkg),
                Opt::Var(_) => None,
            })
            .sorted_unstable_by_key(|o| o.pkg.clone())
        {
            let PkgOpt {
                pkg, components, ..
            } = requirement;
            if components.is_empty() {
                continue;
            }
            hasher.update(pkg.as_bytes());
            hasher.update(b"=");
            for component in components.iter() {
                // It is not possible to have a custom named component with
                // the same name as a reserved name, so taking the stringified
                // name is enough to ensure uniqueness.
                hasher.update(component.as_str().as_bytes());
                hasher.update(b",");
            }
            hasher.update(&[1]);
        }
        let digest = hasher.finish();
        Ok(BuildId::new_from_bytes(digest.as_ref()))
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

        // Calculating the build ids of the variants here would require having
        // access to the package name, what option overrides are in effect,
        // if host options are are disabled, or what the host options are.
        //
        // Instead of comparing via build id, we just compare the variant
        // content to check that they are unique.

        let mut unique_variants = HashMap::new();
        for variant in bs.variants.iter() {
            let variant_uniqueness_key = {
                // OptionMaps are already sorted.
                let options = variant.options();
                // Sort the additional requirements so two variants with the
                // same requirements but in a different order are still
                // considered the same.
                let requirements = variant
                    .additional_requirements()
                    .iter()
                    .cloned()
                    .sorted()
                    .collect::<Vec<_>>();
                (options, requirements)
            };
            let variants_with_key = unique_variants
                .entry(variant_uniqueness_key)
                .or_insert_with(Vec::new);
            variants_with_key.push(variant);
            if variants_with_key.len() < 2 {
                continue;
            }
            let details = variants_with_key
                .iter()
                .map(|o| format!("  - {o:#}"))
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
#[derive(Default)]
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

#[derive(Default)]
struct BuildSpecVisitor {
    build_spec: UncheckedBuildSpec,
    lints: Vec<Lint>,
}

impl Lints for BuildSpecVisitor {
    fn lints(&mut self) -> Vec<Lint> {
        std::mem::take(&mut self.lints)
    }
}

impl From<BuildSpecVisitor> for UncheckedBuildSpec {
    fn from(value: BuildSpecVisitor) -> Self {
        value.build_spec
    }
}

impl<'de> Deserialize<'de> for UncheckedBuildSpec {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        Ok(deserializer
            .deserialize_map(BuildSpecVisitor::default())?
            .into())
    }
}

impl<'de> Deserialize<'de> for LintedItem<UncheckedBuildSpec> {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        Ok(deserializer
            .deserialize_map(BuildSpecVisitor::default())?
            .into())
    }
}

impl<'de> serde::de::Visitor<'de> for BuildSpecVisitor {
    type Value = BuildSpecVisitor;

    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("a build specification")
    }

    fn visit_map<A>(mut self, mut map: A) -> std::result::Result<Self::Value, A::Error>
    where
        A: serde::de::MapAccess<'de>,
    {
        let mut variants = Vec::<v0::VariantSpec>::new();
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
                "validation" => unchecked.validation = map.next_value::<ValidationSpec>()?,
                "auto_host_vars" => unchecked.auto_host_vars = map.next_value::<AutoHostVars>()?,
                unknown_key => {
                    self.lints.push(Lint::Key(UnknownKey::new(
                        unknown_key,
                        BuildSpec::FIELD_NAMES_AS_ARRAY.to_vec(),
                    )));
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

        Ok(Self {
            build_spec: UncheckedBuildSpec(unchecked),
            lints: self.lints,
        })
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
