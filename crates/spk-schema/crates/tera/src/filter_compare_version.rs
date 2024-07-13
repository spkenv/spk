// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::str::FromStr;

use serde_json::Value;
use spk_schema_foundation::version::Version;
use spk_schema_foundation::version_range::{Ranged, VersionFilter};

#[cfg(test)]
#[path = "./filter_compare_version_test.rs"]
mod filter_compare_version_test;

/// Compares one version to another using spk ordering semantics
pub struct CompareVersion;

impl CompareVersion {
    pub const FILTER_NAME: &'static str = "compare_version";
    /// The comparison operation to perform
    pub const ARG_OPERATOR: &'static str = "op";
    /// The version to compare with, if not part of the operator string
    pub const ARG_RHS: &'static str = "rhs";
    pub const ARGS: &'static [&'static str] = &[Self::ARG_OPERATOR, Self::ARG_RHS];
}

impl tera::Filter for CompareVersion {
    fn filter(
        &self,
        value: &Value,
        args: &std::collections::HashMap<String, Value>,
    ) -> tera::Result<Value> {
        let Some(range_str) = args.get(Self::ARG_OPERATOR) else {
            return Err(tera::Error::msg(format!(
                "{}: missing required argument {:?}",
                Self::FILTER_NAME,
                Self::ARG_OPERATOR,
            )));
        };
        let Value::String(mut range_str) = range_str.clone() else {
            return Err(tera::Error::msg(format!(
                "{}: {} argument expected a string value, got {range_str:?}",
                Self::FILTER_NAME,
                Self::ARG_OPERATOR
            )));
        };
        if let Some(rhs) = args.get(Self::ARG_RHS) {
            if rhs.is_array() || rhs.is_object() {
                return Err(tera::Error::msg(format!(
                    "{}: {} argument expected a scalar value, got {rhs:?}",
                    Self::FILTER_NAME,
                    Self::ARG_RHS
                )));
            }
            range_str.push_str(&rhs.to_string());
        };

        match args.len() {
            1 => {}
            2 if args.contains_key(Self::ARG_RHS) => {}
            _ => {
                return Err(tera::Error::msg(format!(
                    "{}: one or more unsupported arguments provided, supported args: {:?}",
                    Self::FILTER_NAME,
                    Self::ARGS
                )));
            }
        }

        let Value::String(value) = value else {
            return Err(tera::Error::msg(format!(
                "{}: expected string input, got {:?}",
                Self::FILTER_NAME,
                value,
            )));
        };

        let lhs = Version::from_str(value)
            .map_err(|err| tera::Error::chain("invalid version for compare_version", err))?;
        let range = VersionFilter::from_str(range_str.as_str())
            .map_err(|err| tera::Error::chain("invalid comparison string", err))?;

        let result = range.is_applicable(&lhs).is_ok();
        Ok(result.into())
    }
}
