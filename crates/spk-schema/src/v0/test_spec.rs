// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use serde::{Deserialize, Serialize};
use spk_schema_foundation::option_map::Stringified;
use struct_field_names_as_array::FieldNamesAsArray;

use crate::ident::Request;
use crate::{Lint, LintedItem, Lints, Script, TestStage, UnknownKey};

#[cfg(test)]
#[path = "./test_spec_test.rs"]
mod test_spec_test;

/// A set of structured inputs used to build a package.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[cfg_attr(test, serde(deny_unknown_fields))]
pub struct TestSpec {
    pub stage: TestStage,
    pub script: Script,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub selectors: Vec<super::VariantSpec>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requirements: Vec<Request>,
}

impl crate::Test for TestSpec {
    fn script(&self) -> String {
        self.script.join("\n")
    }

    fn additional_requirements(&self) -> Vec<Request> {
        self.requirements.clone()
    }
}

#[derive(Default, FieldNamesAsArray)]
struct TestSpecVisitor {
    stage: Option<TestStage>,
    script: Option<Script>,
    selectors: Vec<super::VariantSpec>,
    requirements: Vec<Request>,
    #[field_names_as_array(skip)]
    lints: Vec<Lint>,
}

impl Lints for TestSpecVisitor {
    fn lints(&mut self) -> Vec<Lint> {
        std::mem::take(&mut self.lints)
    }
}

impl From<TestSpecVisitor> for TestSpec {
    fn from(value: TestSpecVisitor) -> Self {
        Self {
            stage: value.stage.expect("a stage"),
            script: value.script.expect("a script"),
            selectors: value.selectors,
            requirements: value.requirements,
        }
    }
}

impl<'de> Deserialize<'de> for TestSpec {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        Ok(deserializer
            .deserialize_map(TestSpecVisitor::default())?
            .into())
    }
}

impl<'de> Deserialize<'de> for LintedItem<TestSpec> {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        Ok(deserializer
            .deserialize_map(TestSpecVisitor::default())?
            .into())
    }
}

impl<'de> serde::de::Visitor<'de> for TestSpecVisitor {
    type Value = TestSpecVisitor;

    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("a test specification")
    }

    fn visit_map<A>(mut self, mut map: A) -> std::result::Result<Self::Value, A::Error>
    where
        A: serde::de::MapAccess<'de>,
    {
        while let Some(key) = map.next_key::<Stringified>()? {
            match key.as_str() {
                "stage" => self.stage = Some(map.next_value::<TestStage>()?),
                "script" => self.script = Some(map.next_value::<Script>()?),
                "selectors" => self.selectors = map.next_value::<Vec<super::VariantSpec>>()?,
                "requirements" => self.requirements = map.next_value::<Vec<Request>>()?,
                unknown_key => {
                    self.lints.push(Lint::Key(UnknownKey::new(
                        unknown_key,
                        TestSpecVisitor::FIELD_NAMES_AS_ARRAY.to_vec(),
                    )));
                    map.next_value::<serde::de::IgnoredAny>()?;
                }
            }
        }

        if self.stage.is_none() || self.script.is_none() {
            return Err(serde::de::Error::custom("Unknown key found"));
        }

        Ok(self)
    }
}
