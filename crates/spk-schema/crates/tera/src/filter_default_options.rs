// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use serde_json::Value;
use spk_schema_foundation::name::OptName;

#[cfg(test)]
#[path = "./filter_default_options_test.rs"]
mod filter_default_options_test;

/// Allows for specifying default options for template execution
#[derive(Clone, Copy)]
pub struct DefaultOpts;

impl DefaultOpts {
    pub const FILTER_NAME: &'static str = "default_opts";
}

impl tera::Filter for DefaultOpts {
    fn filter(
        &self,
        value: &Value,
        args: &std::collections::HashMap<String, Value>,
    ) -> tera::Result<Value> {
        let Value::Object(mut options) = value.clone() else {
            return Err(tera::Error::msg(format!(
                "{}: expected object input, got: {value:?}",
                Self::FILTER_NAME
            )));
        };
        for (name, value) in args {
            OptName::validate(&name)
                .map_err(|err| tera::Error::chain("Could not assign option", err))?;
            let value_str = match value {
                Value::Null => String::new(),
                Value::Bool(b) => b.to_string(),
                Value::Number(n) => n.to_string(),
                Value::String(s) => s.clone(),
                Value::Array(_) | Value::Object(_) => {
                    return Err(tera::Error::msg(format!(
                        "{}: Expected a scalar option value for {name:?}, got: {value:?}",
                        Self::FILTER_NAME
                    )));
                }
            };
            if options.contains_key(name) {
                continue;
            }
            options.insert(name.clone(), value_str.into());
        }
        Ok(options.into())
    }
}
