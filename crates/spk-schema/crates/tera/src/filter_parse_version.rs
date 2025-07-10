// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::ops::Deref;
use std::str::FromStr;

use serde_json::Value;
use spk_schema_foundation::version::Version;

#[cfg(test)]
#[path = "./filter_parse_version_test.rs"]
mod filter_parse_version_test;

pub struct ParseVersion;

impl ParseVersion {
    pub const FILTER_NAME: &'static str = "parse_version";
    pub const FIELD_MAJOR: &'static str = "major";
    pub const FIELD_MINOR: &'static str = "minor";
    pub const FIELD_PATCH: &'static str = "patch";
    pub const FIELD_BASE: &'static str = "base";
    pub const FIELD_PARTS: &'static str = "parts";
    pub const FIELD_EPSILON: &'static str = "epsilon";
    pub const FIELD_PRE: &'static str = "pre";
    pub const FIELD_POST: &'static str = "post";
    pub const FIELDS: &'static [&'static str] = &[
        Self::FIELD_MAJOR,
        Self::FIELD_MINOR,
        Self::FIELD_PATCH,
        Self::FIELD_BASE,
        Self::FIELD_PARTS,
        Self::FIELD_EPSILON,
        Self::FIELD_PRE,
        Self::FIELD_POST,
    ];

    pub const ARG_FIELD: &'static str = "field";
    pub const ARGS: &'static [&'static str] = &[Self::ARG_FIELD];
}

impl tera::Filter for ParseVersion {
    fn filter(
        &self,
        value: &serde_json::Value,
        args: &std::collections::HashMap<String, serde_json::Value>,
    ) -> tera::Result<serde_json::Value> {
        let input = value.as_str().ok_or_else(|| {
            tera::Error::msg(format!(
                "{}: Expected a string value for filter 'parse_version', got {value:?}",
                Self::FILTER_NAME
            ))
        })?;
        let version = Version::from_str(input)
            .map_err(|err| tera::Error::chain("Failed to parse version", err))?;

        let mut data = serde_json::json!({
            Self::FIELD_MAJOR: version.major(),
            Self::FIELD_MINOR: version.minor(),
            Self::FIELD_PATCH: version.patch(),
            Self::FIELD_BASE: version.base_normalized(),
            Self::FIELD_PARTS: version.parts.parts,
            Self::FIELD_EPSILON: version.parts.epsilon,
            Self::FIELD_PRE: version.pre.deref(),
            Self::FIELD_POST: version.post.deref(),
        });

        match args.len() {
            0 => {}
            1 if args.contains_key(Self::ARG_FIELD) => {}
            _ => {
                return Err(tera::Error::msg(format!(
                    "{}: one or more unsupported arguments provided, supported args: {:?}",
                    Self::FILTER_NAME,
                    Self::ARGS
                )));
            }
        }

        if let Some(field) = &args.get(Self::ARG_FIELD) {
            let Value::String(field) = field else {
                return Err(tera::Error::msg(format!(
                    "{}: 'field' argument expected a string, got: {field:?}",
                    Self::FILTER_NAME,
                )));
            };
            let Some(part) = data.as_object_mut().and_then(|o| o.remove(field)) else {
                return Err(tera::Error::msg(format!(
                    "{}: spk version has no field {field:?}, available fields: {:?}",
                    Self::FILTER_NAME,
                    Self::FIELDS
                )));
            };
            return Ok(part);
        }

        Ok(data)
    }
}
