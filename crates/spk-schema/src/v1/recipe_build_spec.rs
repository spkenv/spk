// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fmt::Write;
use std::marker::PhantomData;

use itertools::Itertools;
use serde::{Deserialize, Serialize};
use spk_schema_foundation::option_map::OptionMap;

use super::{ScriptBlock, TestScript};
use crate::RequirementsList;

#[derive(Debug, Default, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, serde(deny_unknown_fields))]
pub struct RecipeBuildSpec {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variants: Vec<VariantSpec>,
    pub script: ScriptBlock,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub test: Vec<TestScript>,

    /// reserved to help avoid common mistakes in production
    #[serde(
        default,
        deserialize_with = "super::source_spec::no_tests_field",
        skip_serializing
    )]
    tests: PhantomData<()>,
    /// reserved to help avoid common mistakes in production
    #[serde(default, deserialize_with = "no_build_options_field", skip_serializing)]
    options: PhantomData<()>,
}

/// Variants are compared and sorted without their name considered,
/// which ensures that they can be deduplicated based on the actual
/// package that they would build.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[cfg_attr(test, serde(deny_unknown_fields))]
pub struct VariantSpec {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub options: OptionMap,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requests: RequirementsList,
}

impl crate::Variant for VariantSpec {
    fn name(&self) -> Option<&str> {
        Some(&self.name)
    }

    fn options(&self) -> Cow<'_, OptionMap> {
        Cow::Borrowed(&self.options)
    }

    fn additional_requirements(&self) -> Cow<'_, crate::RequirementsList> {
        Cow::Borrowed(&self.requests)
    }
}

impl std::fmt::Display for VariantSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.name.fmt(f)?;
        f.write_str(": ")?;
        self.options.fmt(f)?;
        f.write_char('\n')?;
        for request in self.requests.iter() {
            f.write_str(" - ")?;
            f.write_fmt(format_args!("{request:?}"))?;
            f.write_char('\n')?;
        }
        Ok(())
    }
}

impl std::hash::Hash for VariantSpec {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.options.hash(state);
        let sorted: Vec<_> = self.requests.iter().sorted().collect();
        sorted.hash(state);
    }
}

impl std::cmp::PartialEq for VariantSpec {
    fn eq(&self, other: &Self) -> bool {
        self.options == other.options
            && self.requests.iter().sorted().collect::<Vec<_>>()
                == other.requests.iter().sorted().collect::<Vec<_>>()
    }
}

impl std::cmp::Eq for VariantSpec {}

fn no_build_options_field<'de, D>(_deserializer: D) -> Result<PhantomData<()>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Err(serde::de::Error::custom(
        "no field 'build.options'. The 'options' field lives at the root in v1, not under 'build'.",
    ))
}
