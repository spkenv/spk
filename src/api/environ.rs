// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use pyo3::prelude::*;
use serde::{Deserialize, Serialize};

#[cfg(test)]
#[path = "./environ_test.rs"]
mod environ_test;

#[cfg(windows)]
const DEFAULT_VAR_SEP: &str = ";";
#[cfg(unix)]
const DEFAULT_VAR_SEP: &str = ":";

/// An operation performed to the environment
#[derive(Debug, Clone, Hash, Serialize, Eq, PartialEq, FromPyObject)]
#[serde(untagged)]
pub enum EnvOp {
    Append(AppendEnv),
    Prepend(PrependEnv),
    Set(SetEnv),
}

impl EnvOp {
    /// Construct the bash source representation for this operation
    pub fn bash_source(&self) -> String {
        match self {
            Self::Append(op) => op.bash_source(),
            Self::Prepend(op) => op.bash_source(),
            Self::Set(op) => op.bash_source(),
        }
    }

    /// Construct the tcsh source representation for this operation
    pub fn tcsh_source(&self) -> String {
        match self {
            Self::Append(op) => op.tcsh_source(),
            Self::Prepend(op) => op.tcsh_source(),
            Self::Set(op) => op.tcsh_source(),
        }
    }
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
    #[serde(deserialize_with = "super::option_map::string_from_scalar")]
    #[pyo3(get, set)]
    value: String,
    #[serde(
        default,
        deserialize_with = "super::option::optional_string_from_scalar"
    )]
    #[pyo3(get, set)]
    separator: Option<String>,
}

#[pymethods]
impl AppendEnv {
    /// Return the separator for this append operation
    pub fn sep<'a>(&'a self) -> &'a str {
        self.separator
            .as_ref()
            .map(String::as_str)
            .unwrap_or(DEFAULT_VAR_SEP)
    }

    /// Construct the bash source representation for this operation
    pub fn bash_source(&self) -> String {
        format!(
            "export {}=\"${{{}}}{}{}\"",
            self.append,
            self.append,
            self.sep(),
            self.value
        )
    }
    /// Construct the tcsh source representation for this operation
    pub fn tcsh_source(&self) -> String {
        // tcsh will complain if we use a variable that is not defined
        // so there is extra login in here to define it as needed
        vec![
            format!("if ( $?{} ) then", self.append),
            format!(
                "setenv {} \"${{{}}}{}{}\"",
                self.append,
                self.append,
                self.sep(),
                self.value,
            ),
            "else".to_string(),
            format!("setenv {} \"{}\"", self.append, self.value),
            "endif".to_string(),
        ]
        .join("\n")
    }
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
    #[serde(deserialize_with = "super::option_map::string_from_scalar")]
    #[pyo3(get, set)]
    value: String,
    #[serde(
        default,
        deserialize_with = "super::option::optional_string_from_scalar"
    )]
    #[pyo3(get, set)]
    separator: Option<String>,
}

#[pymethods]
impl PrependEnv {
    /// Return the separator for this prepend operation
    pub fn sep<'a>(&'a self) -> &'a str {
        self.separator
            .as_ref()
            .map(String::as_str)
            .unwrap_or(DEFAULT_VAR_SEP)
    }

    /// Construct the bash source representation for this operation
    pub fn bash_source(&self) -> String {
        format!(
            "export {}=\"{}{}${{{}}}\"",
            self.prepend,
            self.value,
            self.sep(),
            self.prepend,
        )
    }
    /// Construct the tcsh source representation for this operation
    pub fn tcsh_source(&self) -> String {
        // tcsh will complain if we use a variable that is not defined
        // so there is extra login in here to define it as needed
        vec![
            format!("if ( $?{} ) then", self.prepend),
            format!(
                "setenv {} \"{}{}${{{}}}\"",
                self.prepend,
                self.value,
                self.sep(),
                self.prepend,
            ),
            "else".to_string(),
            format!("setenv {} \"{}\"", self.prepend, self.value),
            "endif".to_string(),
        ]
        .join("\n")
    }
}

/// Operates on an environment variable by setting it to a value
#[pyclass]
#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetEnv {
    #[pyo3(get, set)]
    set: String,
    #[serde(deserialize_with = "super::option_map::string_from_scalar")]
    #[pyo3(get, set)]
    value: String,
}

#[pymethods]
impl SetEnv {
    /// Construct the bash source representation for this operation
    pub fn bash_source(&self) -> String {
        format!("export {}=\"{}\"", self.set, self.value)
    }
    /// Construct the tcsh source representation for this operation
    pub fn tcsh_source(&self) -> String {
        format!("setenv {} \"{}\"", self.set, self.value)
    }
}
