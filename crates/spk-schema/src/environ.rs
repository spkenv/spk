// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use spk_schema_foundation::option_map::Stringified;
use struct_field_names_as_array::FieldNamesAsArray;

use crate::{Lint, LintedItem, Lints, UnknownKey};

#[cfg(test)]
#[path = "./environ_test.rs"]
mod environ_test;

#[cfg(windows)]
const DEFAULT_VAR_SEP: &str = ";";
#[cfg(unix)]
const DEFAULT_VAR_SEP: &str = ":";

const OP_APPEND: &str = "append";
const OP_COMMENT: &str = "comment";
const OP_PREPEND: &str = "prepend";
const OP_PRIORITY: &str = "priority";
const OP_SET: &str = "set";
const OP_NAMES: &[&str] = &[OP_APPEND, OP_COMMENT, OP_PREPEND, OP_SET];

/// The set of operation types for use in deserialization
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum OpKind {
    Append,
    Comment,
    Prepend,
    Priority,
    Set,
}

/// An operation performed to the environment
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(untagged)]
pub enum EnvOp {
    Append(AppendEnv),
    Comment(EnvComment),
    Prepend(PrependEnv),
    Priority(EnvPriority),
    Set(SetEnv),
}

impl EnvOp {
    pub fn kind(&self) -> OpKind {
        match self {
            EnvOp::Append(_) => OpKind::Append,
            EnvOp::Comment(_) => OpKind::Comment,
            EnvOp::Prepend(_) => OpKind::Prepend,
            EnvOp::Priority(_) => OpKind::Priority,
            EnvOp::Set(_) => OpKind::Set,
        }
    }

    pub fn error(&self) -> &str {
        match self {
            EnvOp::Append(_) => "",
            EnvOp::Comment(_) => "",
            EnvOp::Prepend(_) => "",
            EnvOp::Priority(_) => "",
            EnvOp::Set(_) => "",
        }
    }

    /// Construct the source representation for this operation in the
    /// format of the identified shell.
    pub fn source_for_shell(&self, shell: spfs::ShellKind) -> String {
        match shell {
            spfs::ShellKind::Bash => self.bash_source(),
            spfs::ShellKind::Tcsh => self.tcsh_source(),
            spfs::ShellKind::Powershell => self.powershell_source(),
        }
    }

    /// The environment priority assigned by this operation, if any
    pub fn priority(&self) -> Option<u8> {
        match self {
            Self::Append(_) => None,
            Self::Comment(_) => None,
            Self::Prepend(_) => None,
            Self::Priority(op) => Some(op.priority()),
            Self::Set(_) => None,
        }
    }

    /// Returns a new EnvOp object with the expanded value
    pub fn update_value(&self, expanded_val: String) -> Self {
        match self {
            Self::Prepend(op) => EnvOp::Prepend(PrependEnv {
                prepend: op.prepend.clone(),
                separator: op.separator.clone(),
                value: expanded_val,
            }),
            Self::Append(op) => EnvOp::Append(AppendEnv {
                append: op.append.clone(),
                separator: op.separator.clone(),
                value: expanded_val,
            }),
            Self::Set(op) => EnvOp::Set(SetEnv {
                set: op.set.clone(),
                value: expanded_val,
            }),
            Self::Comment(_) => self.clone(),
            Self::Priority(_) => self.clone(),
        }
    }

    /// Returns the environment variable value if any
    pub fn value(&self) -> Option<&String> {
        match self {
            Self::Append(op) => Some(&op.value),
            Self::Comment(_) => None,
            Self::Prepend(op) => Some(&op.value),
            Self::Priority(_) => None,
            Self::Set(op) => Some(&op.value),
        }
    }

    /// Returns the EnvOp object with expanded env var, if any
    pub fn to_expanded(&self, env_vars: &HashMap<String, String>) -> Self {
        let value = self
            .value()
            .map(|val| shellexpand::env_with_context_no_errors(val, |s: &str| env_vars.get(s)));

        match value {
            Some(val) => self.update_value(val.into_owned()),
            None => self.clone(),
        }
    }

    /// Construct the bash source representation for this operation
    pub fn bash_source(&self) -> String {
        match self {
            Self::Append(op) => op.bash_source(),
            Self::Comment(op) => op.bash_source(),
            Self::Prepend(op) => op.bash_source(),
            Self::Priority(op) => op.bash_source(),
            Self::Set(op) => op.bash_source(),
        }
    }

    /// Construct the tcsh source representation for this operation
    pub fn tcsh_source(&self) -> String {
        match self {
            Self::Append(op) => op.tcsh_source(),
            Self::Comment(op) => op.tcsh_source(),
            Self::Prepend(op) => op.tcsh_source(),
            Self::Priority(op) => op.tcsh_source(),
            Self::Set(op) => op.tcsh_source(),
        }
    }

    /// Construct the powershell source representation for this operation
    pub fn powershell_source(&self) -> String {
        todo!()
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
enum ConfKind {
    Priority(u8),
    Operation(String),
}

impl ConfKind {
    pub fn get_op(&self) -> String {
        match self {
            ConfKind::Priority(_) => String::from(""),
            ConfKind::Operation(o) => o.clone(),
        }
    }

    pub fn get_priority(&self) -> u8 {
        match self {
            ConfKind::Priority(p) => *p,
            ConfKind::Operation(_) => 0,
        }
    }
}

#[derive(Default, Debug, FieldNamesAsArray)]
struct EnvOpVisitor {
    #[field_names_as_array(skip)]
    op_and_var: Option<(OpKind, ConfKind)>,
    value: Option<String>,
    separator: Option<String>,
    #[field_names_as_array(skip)]
    lints: Vec<Lint>,
}

impl Lints for EnvOpVisitor {
    fn lints(&mut self) -> Vec<Lint> {
        std::mem::take(&mut self.lints)
    }
}

impl From<EnvOpVisitor> for EnvOp {
    fn from(mut value: EnvOpVisitor) -> Self {
        let (op, var) = value.op_and_var.expect("an operation and variable");
        match op {
            OpKind::Prepend => EnvOp::Prepend(PrependEnv {
                prepend: var.get_op(),
                separator: value.separator.take(),
                value: value.value.expect("an environment value"),
            }),
            OpKind::Append => EnvOp::Append(AppendEnv {
                append: var.get_op(),
                separator: value.separator.take(),
                value: value.value.expect("an environment value"),
            }),
            OpKind::Set => EnvOp::Set(SetEnv {
                set: var.get_op(),
                value: value.value.expect("an environment value"),
            }),
            OpKind::Comment => EnvOp::Comment(EnvComment {
                comment: var.get_op(),
            }),
            OpKind::Priority => EnvOp::Priority(EnvPriority {
                priority: var.get_priority(),
            }),
        }
    }
}

impl<'de> Deserialize<'de> for EnvOp {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        Ok(deserializer
            .deserialize_map(EnvOpVisitor::default())?
            .into())
    }
}

impl<'de> Deserialize<'de> for LintedItem<EnvOp> {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        Ok(deserializer
            .deserialize_map(EnvOpVisitor::default())?
            .into())
    }
}

impl<'de> serde::de::Visitor<'de> for EnvOpVisitor {
    type Value = EnvOpVisitor;

    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("an environment operation")
    }

    fn visit_map<A>(mut self, mut map: A) -> std::result::Result<Self::Value, A::Error>
    where
        A: serde::de::MapAccess<'de>,
    {
        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                OP_PREPEND => {
                    self.op_and_var = Some((
                        OpKind::Prepend,
                        ConfKind::Operation(map.next_value::<Stringified>()?.0),
                    ));
                }
                OP_PRIORITY => {
                    self.op_and_var = Some((
                        OpKind::Priority,
                        ConfKind::Priority(map.next_value::<u8>()?),
                    ));
                }
                OP_COMMENT => {
                    self.op_and_var = Some((
                        OpKind::Comment,
                        ConfKind::Operation(map.next_value::<Stringified>()?.0),
                    ));
                }
                OP_APPEND => {
                    self.op_and_var = Some((
                        OpKind::Append,
                        ConfKind::Operation(map.next_value::<Stringified>()?.0),
                    ));
                }
                OP_SET => {
                    self.op_and_var = Some((
                        OpKind::Set,
                        ConfKind::Operation(map.next_value::<Stringified>()?.0),
                    ));
                }
                "value" => self.value = Some(map.next_value::<Stringified>()?.0),
                "separator" => {
                    self.separator = map.next_value::<Option<Stringified>>()?.map(|s| s.0)
                }
                unknown_key => {
                    let mut field_names = EnvOpVisitor::FIELD_NAMES_AS_ARRAY.to_vec();
                    field_names.extend(AppendEnv::FIELD_NAMES_AS_ARRAY.to_vec());
                    field_names.extend(EnvComment::FIELD_NAMES_AS_ARRAY.to_vec());
                    field_names.extend(EnvPriority::FIELD_NAMES_AS_ARRAY.to_vec());
                    field_names.extend(PrependEnv::FIELD_NAMES_AS_ARRAY.to_vec());
                    field_names.extend(SetEnv::FIELD_NAMES_AS_ARRAY.to_vec());
                    self.lints
                        .push(Lint::Key(UnknownKey::new(unknown_key, field_names)));
                    map.next_value::<serde::de::IgnoredAny>()?;
                }
            }
        }

        // Comments and priority configs don't have any values.
        self.value = self
            .op_and_var
            .as_ref()
            .and_then(|(op_kind, _)| match op_kind {
                OpKind::Comment | OpKind::Priority => None,
                _ => Some(
                    self.value
                        .take()
                        .ok_or_else(|| serde::de::Error::missing_field("value")),
                ),
            })
            .transpose()?;

        // Should contain some value by this point
        if self.op_and_var.is_none() {
            return Err(serde::de::Error::custom(format!(
                "missing field to define operation and variable, expected one of {OP_NAMES:?}",
            )));
        }

        Ok(self)
    }
}

/// Operates on an environment variable by appending to the end
///
/// The separator used defaults to the path separator for the current
/// host operating system (':' for unix, ';' for windows)
#[derive(Clone, Debug, Eq, FieldNamesAsArray, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct AppendEnv {
    pub append: String,
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub separator: Option<String>,
}

impl AppendEnv {
    /// Return the separator for this append operation
    pub fn sep(&self) -> &str {
        self.separator.as_deref().unwrap_or(DEFAULT_VAR_SEP)
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
        [
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

/// Adds a comment to the generated environment script
#[derive(Clone, Debug, Eq, FieldNamesAsArray, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct EnvComment {
    pub comment: String,
}

impl EnvComment {
    /// Construct the bash source representation for this operation
    pub fn bash_source(&self) -> String {
        format!("# {}", self.comment)
    }
    /// Construct the tcsh source representation for this operation
    pub fn tcsh_source(&self) -> String {
        // Both bash and tcsh source use the same comment syntax
        self.bash_source()
    }
}

/// Assigns a priority to the generated environment script
#[derive(Clone, Debug, Eq, FieldNamesAsArray, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct EnvPriority {
    pub priority: u8,
}

impl EnvPriority {
    /// Construct the bash source representation for this operation
    pub fn bash_source(&self) -> String {
        String::from("")
    }

    /// Construct the tcsh source representation for this operation
    pub fn tcsh_source(&self) -> String {
        String::from("")
    }

    pub fn priority(&self) -> u8 {
        self.priority
    }
}

/// Operates on an environment variable by prepending to the beginning
///
/// The separator used defaults to the path separator for the current
/// host operating system (':' for unix, ';' for windows)
#[derive(Clone, Debug, Eq, FieldNamesAsArray, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct PrependEnv {
    pub prepend: String,
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub separator: Option<String>,
}

impl PrependEnv {
    /// Return the separator for this prepend operation
    pub fn sep(&self) -> &str {
        self.separator.as_deref().unwrap_or(DEFAULT_VAR_SEP)
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
        [
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
#[derive(Clone, Debug, Eq, FieldNamesAsArray, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct SetEnv {
    pub set: String,
    pub value: String,
}

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
