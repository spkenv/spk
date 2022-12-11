// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::marker::PhantomData;

use serde::{Deserialize, Serialize};
use spk_schema_foundation::option_map::{OptionMap, Stringified};
use spk_schema_foundation::version::Compatibility;
use spk_schema_ident::{NameAndValue, PkgRequest, Satisfy, VarRequest};

use crate::prelude::*;
use crate::BuildEnv;

#[cfg(test)]
#[path = "./when_test.rs"]
mod when_test;

/// Defines when some portion of a recipe should be used.
///
/// The set of conditions in a when block are considered as
/// an 'all' group, meaning that they must all be true in
/// order for the full block to be satisfied.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub enum WhenBlock<Condition = WhenCondition> {
    Always,
    Sometimes { conditions: Vec<Condition> },
}

impl<Condition> Default for WhenBlock<Condition> {
    fn default() -> Self {
        Self::Always
    }
}

impl<Condition> WhenBlock<Condition> {
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
}

impl WhenBlock<WhenCondition> {
    /// Determine if this when block is satisfied by the
    /// given build environment contents. If not satisfied,
    /// the returned compatibility should denote a reason
    /// for the miss.
    pub fn check_is_active<E, P>(&self, build_env: E) -> Compatibility
    where
        E: BuildEnv<Package = P>,
        P: Satisfy<PkgRequest> + Named,
    {
        let conditions = match self {
            Self::Always => return Compatibility::Compatible,
            Self::Sometimes { conditions } => conditions,
        };
        for condition in conditions {
            let compat = condition.check_is_satisfied(&build_env);
            if !compat.is_ok() {
                return compat;
            }
        }
        Compatibility::Compatible
    }
}

impl WhenBlock<VarRequest> {
    /// Determine if this when block is satisfied by the
    /// given build variant. If not satisfied,
    /// the returned compatibility should denote a reason
    /// for the miss.
    pub fn check_is_active_at_build(&self, options: &OptionMap) -> Compatibility {
        let conditions = match self {
            Self::Always => return Compatibility::Compatible,
            Self::Sometimes { conditions } => conditions,
        };
        for condition in conditions {
            if condition.value.is_empty() {
                continue;
            }
            let current = options.get(&condition.var);
            let current = current.map(String::as_str).unwrap_or_default();
            if current.is_empty() {
                return Compatibility::incompatible(format!(
                    "needed {condition}, but no value was set"
                ));
            }

            if current != condition.value {
                return Compatibility::incompatible(format!(
                    "needed {condition}, but got {current:?}"
                ));
            }
        }
        Compatibility::Compatible
    }
}

impl<'de, Condition> Deserialize<'de> for WhenBlock<Condition>
where
    Condition: serde::de::DeserializeOwned,
{
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct WhenBlockVisitor<Condition>(PhantomData<*const Condition>);

        impl<'de, Condition> serde::de::Visitor<'de> for WhenBlockVisitor<Condition>
        where
            Condition: serde::de::DeserializeOwned,
        {
            type Value = WhenBlock<Condition>;

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
                let deserializer = serde::de::value::MapAccessDeserializer::new(map);
                let single = Condition::deserialize(deserializer)?;
                Ok(WhenBlock::Sometimes {
                    conditions: vec![single],
                })
            }
        }

        deserializer.deserialize_any(WhenBlockVisitor(PhantomData))
    }
}

impl<Condition> serde::Serialize for WhenBlock<Condition>
where
    Condition: serde::Serialize,
{
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
    pub fn check_is_satisfied<E, P>(&self, build_env: E) -> Compatibility
    where
        E: BuildEnv<Package = P>,
        P: Satisfy<PkgRequest> + Named,
    {
        let options = build_env.options();
        match self {
            Self::Pkg(req) => {
                let Some(resolved) = build_env.packages().into_iter().find(|p| p.name() == req.pkg.name()) else {
                    return Compatibility::incompatible(format!("pkg: {} is not present in the build environment", req.pkg.name()));
                };
                resolved.check_satisfies_request(req)
            }
            Self::Var(req) => {
                let value = options
                    .get(&req.var)
                    .or_else(|| options.get(req.var.without_namespace()));
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

/// Some portion of a recipe that may be conditional on
/// a when block
#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd, Deserialize, Serialize)]
#[cfg_attr(test, serde(deny_unknown_fields))]
pub struct Conditional<T> {
    #[serde(flatten)]
    inner: T,
    #[serde(default, skip_serializing_if = "WhenBlock::is_always")]
    when: WhenBlock,
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
