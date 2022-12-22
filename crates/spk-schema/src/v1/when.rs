// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::BTreeSet;
use std::marker::PhantomData;

use serde::{Deserialize, Serialize};
use spk_schema_foundation::ident_component::Component;
use spk_schema_foundation::option_map::{OptionMap, Stringified};
use spk_schema_foundation::version::Compatibility;
use spk_schema_ident::{NameAndValue, PkgRequest, Satisfy, VarRequest};

use crate::prelude::*;
use crate::{BuildEnv, BuildEnvMember};

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
    pub fn check_is_active<E>(&self, build_env: E) -> ConditionOutcome
    where
        E: BuildEnv,
        E::Package: Satisfy<PkgRequest> + Named,
    {
        let conditions = match self {
            Self::Always => return ConditionOutcome::always(),
            Self::Sometimes { conditions } => conditions,
        };
        conditions
            .iter()
            .map(|condition| condition.check_is_satisfied(&build_env))
            .collect()
    }
}

impl WhenBlock<VarRequest> {
    /// Determine if this when block is satisfied by the
    /// given build variant. If not satisfied,
    /// the returned compatibility should denote a reason
    /// for the miss.
    pub fn check_is_active_at_build(&self, options: &OptionMap) -> ConditionOutcome {
        let conditions = match self {
            Self::Always => return ConditionOutcome::always(),
            Self::Sometimes { conditions } => conditions,
        };
        for condition in conditions {
            if condition.value.is_empty() {
                continue;
            }
            let current = options.get(&condition.var);
            let current = current.map(String::as_str).unwrap_or_default();
            if current.is_empty() {
                return ConditionOutcome::disabled(format!(
                    "needed {condition}, but no value was set"
                ));
            }

            if current != condition.value {
                return ConditionOutcome::disabled(format!(
                    "needed {condition}, but got {current:?}"
                ));
            }
        }
        ConditionOutcome::always()
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
    pub fn check_is_satisfied<E>(&self, build_env: E) -> ConditionOutcome
    where
        E: BuildEnv,
        E::Package: Satisfy<PkgRequest> + Named,
    {
        let options = build_env.options();
        let target = build_env.target();
        match self {
            Self::Pkg(req) if req.pkg.name() == target.name() => {
                if let Compatibility::Incompatible { reason } =
                    req.pkg.is_applicable(&target.to_any(None))
                {
                    return ConditionOutcome::disabled(reason);
                }
                if req.pkg.components.is_empty() {
                    ConditionOutcome::always()
                } else {
                    ConditionOutcome::Enabled {
                        for_components: Some(req.pkg.components.clone()),
                    }
                }
            }
            Self::Pkg(req) => {
                let Some(resolved) = build_env.get_member(req.pkg.name()) else {
                    return ConditionOutcome::disabled(format!("pkg: {} is not present in the build environment", req.pkg.name()));
                };
                match resolved.package().check_satisfies_request(req) {
                    Compatibility::Compatible => ConditionOutcome::always(),
                    Compatibility::Incompatible { reason } => ConditionOutcome::disabled(reason),
                }
            }
            Self::Var(req) => {
                let value = options
                    .get(&req.var)
                    .or_else(|| options.get(req.var.without_namespace()));
                let Some(value) = value else {
                    return ConditionOutcome::disabled(format!("var: {} is not present in the build environment", req.var));
                };
                if value != &req.value {
                    return ConditionOutcome::disabled(format!(
                        "needed {req}, but the value was {value}"
                    ));
                }
                ConditionOutcome::always()
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
    pub when: WhenBlock,
}

impl<T> From<T> for Conditional<T> {
    fn from(inner: T) -> Self {
        Self {
            inner,
            when: WhenBlock::Always,
        }
    }
}

impl<T> Conditional<T> {
    pub fn into_inner(self) -> T {
        self.inner
    }
}

impl<T> AsRef<T> for Conditional<T> {
    fn as_ref(&self) -> &T {
        &self.inner
    }
}

impl<T> std::ops::Deref for Conditional<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T> std::ops::DerefMut for Conditional<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
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
                    let v = map.next_value::<NameAndValue>()?;
                    result.replace(WhenCondition::Var(VarRequest {
                        var: v.name().clone(),
                        value: v.value_or_default().to_owned(),
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

#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[cfg_attr(test, serde(deny_unknown_fields))]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum ConditionOutcome {
    Disabled {
        reason: String,
    },
    Enabled {
        /// The components for which this condition is active.
        ///
        /// For example, when building 'my-package', an option
        /// can be included with the condition 'my-package:run'
        /// in which case the condition is only enabled for the
        /// 'run' component of 'my-package'
        for_components: Option<BTreeSet<Component>>,
    },
}

impl ConditionOutcome {
    /// Create a disabled variant with the specified reason
    pub fn disabled<T: ToString>(reason: T) -> Self {
        Self::Disabled {
            reason: reason.to_string(),
        }
    }

    /// A condition outcome that denotes being always enabled
    pub fn always() -> Self {
        Self::Enabled {
            for_components: None,
        }
    }

    pub fn is_enabled_for_any(&self) -> bool {
        matches!(self, Self::Enabled { .. })
    }

    pub fn is_enabled_for<'a, I>(&self, components: I) -> bool
    where
        I: IntoIterator<Item = &'a Component>,
    {
        let Self::Enabled { for_components } = self else {
            return false;
        };
        let Some(for_components) = for_components else {
            return true;
        };
        components.into_iter().any(|c| for_components.contains(c))
    }

    pub fn is_disabled(&self) -> bool {
        matches!(self, Self::Disabled { .. })
    }
}

impl FromIterator<Self> for ConditionOutcome {
    fn from_iter<T: IntoIterator<Item = Self>>(iter: T) -> Self {
        let mut out = None;
        for outcome in iter.into_iter() {
            match (&mut out, outcome) {
                (None, outcome) => {
                    out = Some(outcome);
                }
                (Some(Self::Disabled { .. }), _) => {
                    // only the first negative result is saved
                }
                (Some(Self::Enabled { .. }), outcome @ Self::Disabled { .. }) => {
                    // conditions are treated like an 'all'
                    out = Some(outcome);
                    break;
                }
                (
                    Some(Self::Enabled { for_components: a }),
                    Self::Enabled { for_components: b },
                ) => {
                    match (a.as_mut(), b) {
                        (None, None) => {}
                        (None, Some(c)) => *a = Some(c),
                        (Some(_), None) => {}
                        // multiple conditions with different components are combined
                        // as an intersection, where only conditions that appear in both
                        // are valid (since the others are negated by one of the conditions)
                        (Some(a), Some(b)) => a.retain(|c| b.contains(c)),
                    };
                }
            }
        }
        out.unwrap_or_default()
    }
}

impl Default for ConditionOutcome {
    fn default() -> Self {
        Self::Enabled {
            for_components: None,
        }
    }
}
