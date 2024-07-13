// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use serde_json::Value;

#[cfg(test)]
#[path = "./filter_replace_regex_test.rs"]
mod filter_replace_regex_test;

pub struct ReplaceRegex;

impl ReplaceRegex {
    pub const FILTER_NAME: &'static str = "replace_regex";

    /// The regex to search for
    pub const ARG_FROM: &'static str = "from";
    /// The string to replace with
    pub const ARG_TO: &'static str = "to";
    pub const ARGS: &'static [&'static str] = &[Self::ARG_FROM, Self::ARG_TO];
}

impl tera::Filter for ReplaceRegex {
    fn filter(
        &self,
        value: &Value,
        args: &std::collections::HashMap<String, Value>,
    ) -> tera::Result<Value> {
        let Some(regex_str) = args.get(Self::ARG_FROM) else {
            return Err(tera::Error::msg(format!(
                "{}: missing required argument {:?}",
                Self::FILTER_NAME,
                Self::ARG_FROM,
            )));
        };
        let Value::String(regex_str) = regex_str.clone() else {
            return Err(tera::Error::msg(format!(
                "{}: {} argument expected a string value, got {regex_str:?}",
                Self::FILTER_NAME,
                Self::ARG_FROM
            )));
        };

        let replace_str = args
            .get(Self::ARG_TO)
            .cloned()
            .unwrap_or_else(|| Value::String(String::new()));
        let Value::String(replace_str) = replace_str else {
            return Err(tera::Error::msg(format!(
                "{}: {} argument expected a string value, got {replace_str:?}",
                Self::FILTER_NAME,
                Self::ARG_TO
            )));
        };

        if args.len() > Self::ARGS.len() {
            return Err(tera::Error::msg(format!(
                "{}: one or more unsupported arguments provided, supported args: {:?}",
                Self::FILTER_NAME,
                Self::ARGS
            )));
        }

        let Value::String(value) = value else {
            return Err(tera::Error::msg(format!(
                "{}: expected string input, got {:?}",
                Self::FILTER_NAME,
                value,
            )));
        };

        let search = regex::Regex::new(&regex_str)
            .map_err(|err| tera::Error::chain("Invalid regular expression", err))?;

        Ok(Value::String(
            search.replace_all(value, replace_str.as_str()).to_string(),
        ))
    }
}
