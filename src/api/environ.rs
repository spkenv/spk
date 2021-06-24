// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use pyo3::prelude::*;
use serde::{Deserialize, Serialize};

#[cfg(test)]
#[path = "./environ_test.rs"]
mod environ_test;

/// An operation performed to the environment
#[derive(Debug, Clone, Hash, Serialize, Eq, PartialEq, FromPyObject)]
#[serde(untagged)]
pub enum EnvOp {
    Append(AppendEnv),
    Prepend(PrependEnv),
    Set(SetEnv),
}

impl<'de> Deserialize<'de> for EnvOp {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde_yaml::Value;
        let value = Value::deserialize(deserializer)?;
        let mapping = match value {
            Value::Mapping(m) => m,
            _ => return Err(serde::de::Error::custom("expected mapping")),
        };
        if mapping.get(&Value::String("prepend".to_string())).is_some() {
            Ok(EnvOp::Prepend(
                PrependEnv::deserialize(Value::Mapping(mapping))
                    .map_err(|e| serde::de::Error::custom(format!("{:?}", e)))?,
            ))
        } else if mapping.get(&Value::String("append".to_string())).is_some() {
            Ok(EnvOp::Append(
                AppendEnv::deserialize(Value::Mapping(mapping))
                    .map_err(|e| serde::de::Error::custom(format!("{:?}", e)))?,
            ))
        } else if mapping.get(&Value::String("set".to_string())).is_some() {
            Ok(EnvOp::Set(
                SetEnv::deserialize(Value::Mapping(mapping))
                    .map_err(|e| serde::de::Error::custom(format!("{:?}", e)))?,
            ))
        } else {
            Err(serde::de::Error::custom(
                "failed to determine operation type: must have one of 'append', 'prepend' or 'set' field",
            ))
        }
    }
}

/// Operates on an environment variable by appending to the end
///
/// The separator used defaults to the path separator for the current
/// host operating system (':' for unix, ';' for windows)
#[pyclass]
#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppendEnv {
    #[pyo3(get, set)]
    append: String,
    #[pyo3(get, set)]
    value: String,
    #[pyo3(get, set)]
    separator: Option<String>,
}

/// Operates on an environment variable by prepending to the beginning
///
/// The separator used defaults to the path separator for the current
/// host operating system (':' for unix, ';' for windows)
#[pyclass]
#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrependEnv {
    #[pyo3(get, set)]
    prepend: String,
    #[pyo3(get, set)]
    value: String,
    #[pyo3(get, set)]
    separator: Option<String>,
}

/// Operates on an environment variable by setting it to a value
#[pyclass]
#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetEnv {
    #[pyo3(get, set)]
    set: String,
    #[pyo3(get, set)]
    value: String,
}
