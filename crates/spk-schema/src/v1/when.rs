// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use serde::{Deserialize, Serialize};
use spk_schema_foundation::option_map::{OptionMap, Stringified};
use spk_schema_foundation::version::Compatibility;
use spk_schema_ident::{NameAndValue, PkgRequest, Satisfy, VarRequest};

use crate::prelude::*;

#[cfg(test)]
#[path = "./when_test.rs"]
mod when_test;

/// Defines when some portion of a recipe should be used.
///
/// The set of conditions in a when block are considered as
/// an 'all' group, meaning that they must all be true in
/// order for the full block to be satisfied.
#[derive(Clone, Default, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub enum WhenBlock {
    #[default]
    Always,
    Sometimes {
        conditions: Vec<WhenCondition>,
    },
}

impl WhenBlock {
    /// Create a when block that only activates
    /// when it is also requested by some other source
    pub fn when_requested() -> Self {
        Self::Sometimes {
            conditions: Vec::new(),
        }
    }

    /// True if this is the [`Self::Always`] variant.
    pub fn is_always(&self) -> bool {
        matches!(self, Self::Always)
    }

    /// Determine if this when block is satisfied by the
    /// given build environment contents. If not satisfied,
    /// the returned compatibility should denote a reason
    /// for the miss.
    pub fn check_is_active<'a, I, P>(
        &self,
        build_options: &OptionMap,
        build_env: I,
    ) -> Compatibility
    where
        I: IntoIterator<Item = P>,
        P: Satisfy<PkgRequest> + Named,
    {
        let conditions = match self {
            Self::Always => return Compatibility::Compatible,
            Self::Sometimes { conditions } => conditions,
        };
        let build_env = Vec::from_iter(build_env);
        for condition in conditions {
            let compat = condition.check_is_satisfied(build_options, &build_env);
            if !compat.is_ok() {
                return compat;
            }
        }
        Compatibility::Compatible
    }
}

impl<'de> Deserialize<'de> for WhenBlock {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct WhenBlockVisitor;

        impl<'de> serde::de::Visitor<'de> for WhenBlockVisitor {
            type Value = WhenBlock;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("one of 'Requested', 'Always', or a sequence of conditions")
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match v {
                    "Always" => Ok(WhenBlock::Always),
                    "Requested" => Ok(WhenBlock::Sometimes {
                        conditions: Vec::new(),
                    }),
                    _ => Err(serde::de::Error::unknown_variant(
                        v,
                        &["Always", "Requested", "a sequence of conditions"],
                    )),
                }
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let size_hint = seq.size_hint().unwrap_or_default();
                let mut conditions = Vec::with_capacity(size_hint);
                while let Some(condition) = seq.next_element()? {
                    conditions.push(condition);
                }
                Ok(WhenBlock::Sometimes { conditions })
            }

            fn visit_map<A>(self, map: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                let single = WhenConditionVisitor.visit_map(map)?;
                Ok(WhenBlock::Sometimes {
                    conditions: vec![single],
                })
            }
        }

        deserializer.deserialize_any(WhenBlockVisitor)
    }
}

impl serde::Serialize for WhenBlock {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Self::Always => serializer.serialize_str("Always"),
            Self::Sometimes { conditions } if conditions.is_empty() => {
                serializer.serialize_str("Requested")
            }
            Self::Sometimes { conditions } => conditions.serialize(serializer),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd, Serialize)]
#[serde(untagged)]
pub enum WhenCondition {
    Pkg(PkgRequest),
    Var(VarRequest),
}

impl WhenCondition {
    /// Determine if this condition is satisfied by the
    /// given build environment contents. If not satisfied,
    /// the returned compatibility should denote a reason
    /// for the miss.
    pub fn check_is_satisfied<'a, I, P>(
        &self,
        build_options: &OptionMap,
        build_env: I,
    ) -> Compatibility
    where
        I: IntoIterator<Item = P>,
        P: Satisfy<PkgRequest> + Named,
    {
        match self {
            Self::Pkg(req) => {
                let Some(resolved) = build_env.into_iter().find(|p| p.name() == req.pkg.name()) else {
                    return Compatibility::incompatible(format!("pkg: {} is not present in the build environment", req.pkg.name()));
                };
                resolved.check_satisfies_request(req)
            }
            Self::Var(req) => {
                let value = build_options
                    .get(&req.var)
                    .or_else(|| build_options.get(req.var.without_namespace()));
                let Some(value) = value else {
                    return Compatibility::incompatible(format!("var: {} is not present in the build environment", req.var));
                };
                if value != &req.value {
                    return Compatibility::incompatible(format!(
                        "needed {req}, but the value was {value}"
                    ));
                }
                Compatibility::Compatible
            }
        }
    }
}

impl<'de> Deserialize<'de> for WhenCondition {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_any(WhenConditionVisitor)
    }
}

struct WhenConditionVisitor;

impl<'de> serde::de::Visitor<'de> for WhenConditionVisitor {
    type Value = WhenCondition;

    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("a when condition")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::MapAccess<'de>,
    {
        let mut result = None;
        while let Some(key) = map.next_key::<Stringified>()? {
            let previous = match key.as_str() {
                "pkg" => result.replace(WhenCondition::Pkg(PkgRequest::new(
                    map.next_value()?,
                    spk_schema_ident::RequestedBy::DoesNotMatter,
                ))),
                "var" => {
                    let NameAndValue(var, value) = map.next_value()?;
                    result.replace(WhenCondition::Var(VarRequest {
                        var,
                        value: value.unwrap_or_default(),
                        pin: false,
                    }))
                }
                #[cfg(not(test))]
                _name => {
                    // unrecognized fields are explicitly ignored in case
                    // they were added in a newer version of spk. We assume
                    // that if the api has not been versioned then the desire
                    // is to continue working in this older version
                    map.next_value::<serde::de::IgnoredAny>()?;
                    None
                }
                #[cfg(test)]
                name => {
                    // except during testing, where we don't want to hide
                    // failing tests because of ignored data
                    return Err(serde::de::Error::unknown_field(name, &[]));
                }
            };
            if previous.is_some() {
                return Err(serde::de::Error::custom(
                    "multiple conditions found in a single map, was this meant to be a list?",
                ));
            }
        }
        result.ok_or_else(|| serde::de::Error::missing_field("pkg\" or \"var"))
    }
}
