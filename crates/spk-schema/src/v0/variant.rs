// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::borrow::Cow;
use std::collections::BTreeSet;
use std::fmt::Write;
use std::str::FromStr;

use serde::Serialize;
use spk_schema_foundation::name::PkgName;
use spk_schema_foundation::option_map::OptionMap;
use spk_schema_foundation::version_range::{VersionFilter, VersionRange};
use spk_schema_ident::{PkgRequest, RangeIdent, VarRequest};

use super::variant_spec::VariantSpecEntryKey;
use super::VariantSpec;
use crate::{Error, Opt, RequirementsList, Result};

/// A simple build variant used by v0 recipes.
///
/// Typically, these are loaded in a v0 recipe and constructed
/// using an option map based on some heuristics which create
/// requests from those options.
#[derive(Debug, Clone, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct Variant {
    #[serde(flatten)]
    options: OptionMap,
    #[serde(skip)]
    requirements: RequirementsList,
}

impl Variant {
    /// Construct a build variant using the provided build options.
    /// Only the options that have a default value are included in
    /// the variant.
    pub fn from_build_options(build_options: &[Opt], options: &OptionMap) -> Self {
        let options = build_options
            .iter()
            .filter_map(|o| {
                let Opt::Var(o) = o else {
                    return None;
                };
                (!o.default.is_empty()).then(|| {
                    (
                        o.var.clone(),
                        options
                            .get(&o.var)
                            .cloned()
                            .unwrap_or_else(|| o.default.clone()),
                    )
                })
            })
            .collect();
        Self {
            options,
            requirements: RequirementsList::default(),
        }
    }

    /// Construct a build variant using a set of provided options and
    /// the known build options for the package. This uses a set of
    /// heuristics to identify and determine which type of additional
    /// requirements are being specified (if any).
    pub fn from_spec(spec: VariantSpec, build_options: &[Opt]) -> Result<Self> {
        let mut options = OptionMap::default();
        let mut requirements = RequirementsList::default();

        for (key, value) in spec.entries.into_iter() {
            let name = match &key {
                VariantSpecEntryKey::PkgOrOpt(pkg) => pkg.0.name.as_opt_name().to_owned(),
                VariantSpecEntryKey::Opt(opt) => opt.to_owned(),
            };
            let value = value.as_str();
            options.insert(name.clone(), value.to_owned());

            // if it was parsed as something with components, then it is a pkg
            // request.
            if let VariantSpecEntryKey::PkgOrOpt(pkg) = key {
                if !pkg.0.components.is_empty() {
                    let Ok(version_range) = VersionRange::from_str(value) else {
                        return Err(Error::String(format!(
                            "invalid version range for package '{}': {}",
                            name, value
                        )));
                    };
                    requirements.insert_or_replace(
                        PkgRequest::new(
                            RangeIdent {
                                name: pkg.0.name,
                                version: VersionFilter::single(version_range),
                                repository_name: None,
                                components: pkg.0.components.into_inner(),
                                build: None,
                            },
                            spk_schema_ident::RequestedBy::Variant,
                        )
                        .into(),
                    );
                    continue;
                }
            }

            // only items that don't already exist in the build
            // options are considered additional requirements
            if build_options.iter().any(|o| o.full_name() == name) {
                continue;
            }

            // Some heuristics to decide if the variant entry is
            // a var or a pkg...
            //
            // If it is not a valid package name, assume it is a var.
            let Ok(pkg_name) = PkgName::new(name.as_str()) else {
                requirements
                    .insert_or_replace(VarRequest::new_with_value(name.clone(), value).into());
                continue;
            };
            // If the value is not a legal version range, assume it is
            // a var.
            let Ok(version_range) = VersionRange::from_str(value) else {
                requirements
                    .insert_or_replace(VarRequest::new_with_value(name.clone(), value).into());
                continue;
            };
            // It is a valid package name and the value is a legal
            // version range expression, and it doesn't match any
            // declared options. Treat as a new package request
            requirements.insert_or_replace(
                PkgRequest::new(
                    RangeIdent {
                        name: pkg_name.to_owned(),
                        version: VersionFilter::single(version_range),
                        repository_name: None,
                        components: BTreeSet::new(),
                        build: None,
                    },
                    spk_schema_ident::RequestedBy::Variant,
                )
                .into(),
            );
        }
        Ok(Self {
            options,
            requirements,
        })
    }
}

impl crate::Variant for Variant {
    fn options(&self) -> Cow<'_, OptionMap> {
        Cow::Borrowed(&self.options)
    }

    fn additional_requirements(&self) -> Cow<'_, RequirementsList> {
        Cow::Borrowed(&self.requirements)
    }
}

impl std::fmt::Display for Variant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let br = if f.alternate() { ' ' } else { '\n' };
        let pad = if f.alternate() { "" } else { "  " };
        f.write_str("options: ")?;
        self.options.fmt(f)?;
        if self.requirements.len() > 0 {
            f.write_fmt(format_args!("{br}additional requirements:{br}"))?;
            for r in self.requirements.iter() {
                f.write_str(pad)?;
                f.write_fmt(format_args!("{r:#}"))?;
                f.write_char(br)?;
            }
        }
        Ok(())
    }
}
