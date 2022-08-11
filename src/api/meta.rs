// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use chrono::offset::Local;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use whoami::username;

#[cfg(test)]
#[path = "./meta_test.rs"]
mod meta_test;

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct Meta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    #[serde(
        default = "Meta::default_license",
        skip_serializing_if = "String::is_empty"
    )]
    pub license: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub labels: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub modified_stack: Vec<ModifiedMetaData>,
}

impl Default for Meta {
    fn default() -> Self {
        Meta {
            description: None,
            homepage: None,
            license: Self::default_license(),
            labels: Default::default(),
            modified_stack: Vec::new(),
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

    pub fn new_modified_meta_data() -> ModifiedMetaData {
        ModifiedMetaData::default()
    }

    pub fn update_modified_time(&mut self, action: &str, comment: String) {
        let timestamp = Local::now().timestamp();
        let data = ModifiedMetaData {
            author: username(),
            action: action.to_owned(),
            comment,
            timestamp,
        };

        match self.modified_stack.is_empty() {
            true => self.modified_stack = vec![data],
            false => self.modified_stack.push(data),
        }
    }

    pub fn get_recent_modified_time(&mut self) -> ModifiedMetaData {
        let mut recent_modified_time: i64 = 0;
        let mut result: ModifiedMetaData = ModifiedMetaData::default();

        for data in &self.modified_stack {
            if data.timestamp > recent_modified_time {
                recent_modified_time = data.timestamp;
                result = data.to_owned();
            };
        }

        result
    }
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct ModifiedMetaData {
    #[serde(skip_serializing_if = "String::is_empty")]
    pub author: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub comment: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub action: String,
    pub timestamp: i64,
}

impl Default for ModifiedMetaData {
    fn default() -> Self {
        ModifiedMetaData {
            author: username(),
            action: String::default(),
            comment: String::default(),
            timestamp: i64::default(),
        }
    }
}
