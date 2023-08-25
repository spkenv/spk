// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::BTreeMap;
use std::process::{Command, Stdio};

use execute::Execute;
use itertools::Itertools;
use serde::{Deserialize, Serialize};

use crate::{Error, Result};

#[cfg(test)]
#[path = "./meta_test.rs"]
mod meta_test;

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
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

    pub fn update_metadata(&mut self, executable: &String) -> Result<i32> {
        let mut command = Command::new(executable);

        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());

        match command.execute_output() {
            Ok(out) => {
                let stdout = match std::str::from_utf8(&out.stdout) {
                    Ok(s) => s,
                    Err(e) => return Err(Error::String(e.to_string())),
                };
                let mut list_of_data = stdout.split('\n').collect_vec();
                list_of_data.retain(|c| !c.is_empty());
                for metadata in list_of_data.iter() {
                    let data = metadata.split(':').collect_vec();
                    tracing::debug!("{}:{}", data[0], data[1]);
                    self.labels
                        .insert(data[0].trim().to_string(), data[1].trim().to_string());
                }
            }
            Err(e) => {
                tracing::warn!("Failed to execute executable: {e}")
            }
        }
        Ok(0)
    }
}
