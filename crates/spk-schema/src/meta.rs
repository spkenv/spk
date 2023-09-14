// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use spk_schema_foundation::option_map::Stringified;

use crate::{LintMessage, LintedItem, Lints, MetaSpecKey};

#[cfg(test)]
#[path = "./meta_test.rs"]
mod meta_test;

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct Meta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    #[serde(
        default = "Meta::default_license",
        skip_serializing_if = "String::is_empty"
    )]
    pub license: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub labels: BTreeMap<String, String>,
}

impl Default for Meta {
    fn default() -> Self {
        Meta {
            description: None,
            homepage: None,
            license: Self::default_license(),
            labels: Default::default(),
        }
    }
}

impl Meta {
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }
    fn default_license() -> String {
        "Unlicensed".into()
    }
}

#[derive(Default)]
struct MetaVisitor {
    description: Option<String>,
    homepage: Option<String>,
    license: String,
    labels: BTreeMap<String, String>,
    lints: Vec<LintMessage>,
}

impl Lints for MetaVisitor {
    fn lints(&mut self) -> Vec<LintMessage> {
        std::mem::take(&mut self.lints)
    }
}

impl From<MetaVisitor> for Meta {
    fn from(value: MetaVisitor) -> Self {
        Self {
            description: value.description,
            homepage: value.homepage,
            license: value.license,
            labels: value.labels,
        }
    }
}

impl<'de> Deserialize<'de> for Meta {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        Ok(deserializer.deserialize_map(MetaVisitor::default())?.into())
    }
}

impl<'de> Deserialize<'de> for LintedItem<Meta> {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        Ok(deserializer.deserialize_map(MetaVisitor::default())?.into())
    }
}

impl<'de> serde::de::Visitor<'de> for MetaVisitor {
    type Value = MetaVisitor;

    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("a meta specification")
    }

    fn visit_map<A>(mut self, mut map: A) -> std::result::Result<Self::Value, A::Error>
    where
        A: serde::de::MapAccess<'de>,
    {
        while let Some(key) = map.next_key::<Stringified>()? {
            match key.as_str() {
                "description" => self.description = map.next_value::<Option<String>>()?,
                "homepage" => self.homepage = map.next_value::<Option<String>>()?,
                "license" => self.license = map.next_value::<Stringified>()?.0,
                "labels" => self.labels = map.next_value::<BTreeMap<String, String>>()?,
                unknown_key => {
                    self.lints
                        .push(LintMessage::UnknownMetaSpecKey(MetaSpecKey::new(
                            unknown_key,
                        )));
                    map.next_value::<serde::de::IgnoredAny>()?;
                }
            }
        }

        Ok(self)
    }
}
