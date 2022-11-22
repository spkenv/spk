// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fmt::Write;

use serde::{Deserialize, Serialize};
use spk_schema_foundation::option_map::OptionMap;
use spk_schema_ident::Request;

use super::{ScriptBlock, TestScript};

#[derive(Debug, Default, Clone, Hash, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(test, serde(deny_unknown_fields))]
pub struct RecipeBuildSpec {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variants: Vec<VariantSpec>,
    pub script: ScriptBlock,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub test: Vec<TestScript>,
}

#[derive(Debug, Default, Clone, Hash, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(test, serde(deny_unknown_fields))]
pub struct VariantSpec {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub options: OptionMap,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requests: Vec<Request>, // TODO: incorporate requests
}

impl crate::Variant for VariantSpec {
    fn name(&self) -> Option<&str> {
        Some(&self.name)
    }

    fn options(&self) -> Cow<'_, OptionMap> {
        Cow::Borrowed(&self.options)
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
