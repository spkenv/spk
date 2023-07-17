// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use serde::{Deserialize, Serialize};
use spk_schema_foundation::option_map::Stringified;

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
    Comment(CommentEnv),
    Prepend(PrependEnv),
    Priority(Priority),
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

    /// Construct the source representation for this operation in the
    /// format of the identified shell.
    pub fn source_for_shell(&self, shell: spfs::ShellKind) -> String {
        match shell {
            spfs::ShellKind::Bash => self.bash_source(),
            spfs::ShellKind::Tcsh => self.tcsh_source(),
        }
    }

    pub fn priority(&self) -> u8 {
        match self {
            Self::Append(_) => 0,
            Self::Comment(_) => 0,
            Self::Prepend(_) => 0,
            Self::Priority(op) => op.priority(),
            Self::Set(_) => 0,
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
}

impl<'de> Deserialize<'de> for EnvOp {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
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

        #[derive(Default)]
        struct EnvOpVisitor {
            op_and_var: Option<(OpKind, ConfKind)>,
            value: Option<String>,
            separator: Option<String>,
        }

        impl<'de> serde::de::Visitor<'de> for EnvOpVisitor {
            type Value = EnvOp;

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
                        _ => {
                            // ignore any unknown field for the sake of
                            // forward compatibility
                            map.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }

                // Comments and priority configs don't have any values.
                let value = match self.op_and_var.as_ref().unwrap().0 {
                    OpKind::Comment | OpKind::Priority => String::from(""),
                    _ => self
                        .value
                        .take()
                        .ok_or_else(|| serde::de::Error::missing_field("value"))?,
                };

                let value = shellexpand::env(&value)
                    .unwrap_or(std::borrow::Cow::Borrowed(""))
                    .to_string();

                match self.op_and_var.take() {
                    Some((op, var)) => match op {
                        OpKind::Prepend => Ok(EnvOp::Prepend(PrependEnv{
                            prepend: var.get_op(),
                            separator: self.separator.take(),
                            value
                        })),
                        OpKind::Append => Ok(EnvOp::Append(AppendEnv{
                            append: var.get_op(),
                            separator: self.separator.take(),
                            value
                        })),
                        OpKind::Set => Ok(EnvOp::Set(SetEnv{
                            set: var.get_op(),
                            value
                        })),
                        OpKind::Comment => Ok(EnvOp::Comment(CommentEnv{
                            comment: var.get_op()
                        })),
                        OpKind::Priority => Ok(EnvOp::Priority(Priority{
                            priority: var.get_priority()
                        }))
                    },
                    None => Err(serde::de::Error::custom(format!(
                        "missing field to define operation and variable, expected one of {OP_NAMES:?}",
                    ))),
                }
            }
        }
        deserializer.deserialize_map(EnvOpVisitor::default())
    }
}

/// Operates on an environment variable by appending to the end
///
/// The separator used defaults to the path separator for the current
/// host operating system (':' for unix, ';' for windows)
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
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

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct CommentEnv {
    pub comment: String,
}

impl CommentEnv {
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

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct Priority {
    pub priority: u8,
}

impl Priority {
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
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
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
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
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
