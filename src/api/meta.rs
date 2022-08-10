// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use chrono::offset::Local;

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
    pub creation_timestamp: i64,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub modified_stack: BTreeMap<String, Vec<i64>>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub comments: BTreeMap<String, Vec<String>>,
}

impl Default for Meta {
    fn default() -> Self {
        Meta {
            description: None,
            homepage: None,
            license: Self::default_license(),
            labels: Default::default(),
            creation_timestamp: i64::default(),
            modified_stack: Default::default(),
            comments: Default::default(),
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

    pub fn update_comments(&mut self, comment: &[String], action: &str) {

        match self.comments.remove(action) {
            None => self.comments.insert(action.into(), comment.to_owned()),
            Some(mut existing_data) => {
                existing_data.extend(comment.to_owned());
                self.comments.insert(action.into(), existing_data.to_vec())
            }
        };
    }

    pub fn update_modified_time(&mut self, action: &str) {

        let timestamp = Local::now().timestamp();
        let data: Vec<i64> = vec![timestamp];

        match self.modified_stack.remove(action) {
            None => self.modified_stack.insert(action.into(), data),
            Some(mut existing_data) => {
                existing_data.extend(&data);
                self.modified_stack.insert(action.into(), existing_data.to_vec())
            }
        };
    }

    pub fn get_recent_modified_time(&mut self) -> (String, i64) {
        
        let mut recent_modified_time: i64 = 0;
        let mut result: (String, i64) = ("".to_string(), 0);

        match self.modified_stack.is_empty() {
            true => result = ("build".to_string(), self.creation_timestamp),
            false => {
                for (command, timestamps) in &self.modified_stack {
                    for timestamp in timestamps {
                        if timestamp > &recent_modified_time {
                            recent_modified_time = *timestamp;
                            result = (command.to_owned(), recent_modified_time);
                        };
                    };
                };
            }
        };

        result
        // match self.modified_stack.is_empty() {
        //     true => match self.comments.is_empty() {
        //         true => return ("Created on".to_string(), "".to_string(), self.creation_timestamp),
        //         false => return ("Created on".to_string(), self.comments.last().unwrap(), self.creation_timestamp)
        //     },
        //     false => match self.comments.is_empty() {
        //         true => return ("Last Modified".to_string(), "".to_string(), self.modified_stack.last().unwrap()),
        //         false => return ("Last Modified".to_string(), self.comments.last().unwrap(), self.modified_stack.last().unwrap())
        //     },
        // }
    }
}
