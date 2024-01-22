// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::BTreeMap;
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};
use spk_config::Metadata;
use spk_schema_foundation::option_map::Stringified;
use struct_field_names_as_array::FieldNamesAsArray;

use crate::{Error, Lint, LintedItem, Lints, Result, UnknownKey};

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

    pub fn update_metadata(&mut self, global_config: &Metadata) -> Result<i32> {
        for config in global_config.global.iter() {
            let cmd = &config.command;
            let Some(executable) = cmd.first() else {
                tracing::warn!("Empty command in global metadata config");
                continue;
            };

            let args = &cmd[1..];

            let mut command = Command::new(executable);
            command.args(args);
            command.stdout(Stdio::piped());
            command.stderr(Stdio::piped());

            match command
                .spawn()
                .map_err(|err| {
                    Error::ProcessSpawnError(
                        format!("error running configured metadata command: {err}").into(),
                    )
                })?
                .wait_with_output()
            {
                Ok(out) => {
                    let json: serde_json::Value = match serde_json::from_reader(&*out.stdout) {
                        Ok(j) => j,
                        Err(e) => {
                            return Err(Error::String(format!("Unable to read json output: {e}")))
                        }
                    };

                    if let Some(map) = json.as_object() {
                        for (k, v) in map {
                            v.as_str()
                                .and_then(|val| self.labels.insert(k.clone(), val.to_string()));
                        }
                    }
                }
                Err(e) => return Err(Error::String(format!("Failed to execute command: {e}"))),
            }
        }
        Ok(0)
    }
}

#[derive(Default, FieldNamesAsArray)]
struct MetaVisitor {
    description: Option<String>,
    homepage: Option<String>,
    license: Option<String>,
    labels: Option<BTreeMap<String, String>>,
    #[field_names_as_array(skip)]
    lints: Vec<Lint>,
}

impl Lints for MetaVisitor {
    fn lints(&mut self) -> Vec<Lint> {
        std::mem::take(&mut self.lints)
    }
}

impl From<MetaVisitor> for Meta {
    fn from(value: MetaVisitor) -> Self {
        Self {
            description: value.description,
            homepage: value.homepage,
            license: value.license.unwrap_or(Meta::default_license()),
            labels: value.labels.unwrap_or_default(),
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
                "description" => self.description = Some(map.next_value::<Stringified>()?.0),
                "homepage" => self.homepage = Some(map.next_value::<Stringified>()?.0),
                "license" => self.license = Some(map.next_value::<Stringified>()?.0),
                "labels" => self.labels = Some(map.next_value::<BTreeMap<String, String>>()?),
                unknown_key => {
                    self.lints.push(Lint::Key(UnknownKey::new(
                        unknown_key,
                        MetaVisitor::FIELD_NAMES_AS_ARRAY.to_vec(),
                    )));
                    map.next_value::<serde::de::IgnoredAny>()?;
                }
            }
        }

        Ok(self)
    }
}
