// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::collections::{HashMap, HashSet};
use std::hash::Hash;

use itertools::Itertools;
use serde::{Deserialize, Serialize};
use spk_schema_foundation::IsDefault;
use spk_schema_foundation::ident_build::BuildId;
use spk_schema_foundation::name::PkgName;
use spk_schema_foundation::option_map::{OptionMap, Stringified};

use super::{Opt, v0};
use crate::option::PkgOpt;
use crate::v0::RecipeBuildSpec;
use crate::{Error, Result, Script, Variant};

#[cfg(test)]
#[path = "./build_spec_test.rs"]
mod build_spec_test;

/// A set of structured inputs used to build a package.
///
/// This represents the `build` section of a built package. See
/// [`crate::v0::RecipeBuildSpec`] for the type used by recipes.
#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct BuildSpec {
    pub script: Script,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<Opt>,
}

impl BuildSpec {
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

impl IsDefault for BuildSpec {
    fn is_default(&self) -> bool {
        self == &Self::default()
    }
}

impl From<v0::EmbeddedBuildSpec> for BuildSpec {
    fn from(value: v0::EmbeddedBuildSpec) -> Self {
        BuildSpec {
            options: value.options,
            // Other fields are not part of the embedded build spec
            script: Script::default(),
        }
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

impl From<RecipeBuildSpec> for BuildSpec {
    fn from(value: RecipeBuildSpec) -> Self {
        BuildSpec {
            script: value.script,
            options: value.options,
        }
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
