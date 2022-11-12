// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use spk_schema_foundation::ident_component::Component;
use spk_schema_foundation::name::OptNameBuf;
use spk_schema_foundation::option_map::Stringified;
use spk_schema_ident::{NameAndValue, RangeIdent, VarRequest};

#[cfg(test)]
#[path = "./recipe_option_test.rs"]
mod recipe_option_test;

#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VarOption {
    pub var: NameAndValue<OptNameBuf>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub choices: Vec<String>,
    #[serde(default, skip_serializing_if = "VarPropagation::is_default")]
    pub at_build: VarPropagation,
    #[serde(default, skip_serializing_if = "VarPropagation::is_default")]
    pub at_runtime: VarPropagation,
    #[serde(default, skip_serializing_if = "VarPropagation::is_default")]
    pub at_downstream_build: VarPropagation,
    #[serde(default, skip_serializing_if = "VarPropagation::is_default")]
    pub at_downstream_runtime: VarPropagation,
    #[serde(skip_serializing_if = "WhenBlock::is_always")]
    pub when: WhenBlock,
}

/// This visitor is partial because it expects that the first
/// 'var' field has already been partially read. That is, the
/// key has been seen and validated, and so this visitor will
/// continue by reading the value of that field. In all other
/// cases, this will cause the deserializer to fail, and so
/// this type should not be used outside of the specific use
/// case of this module.
struct PartialVarVisitor;

impl<'de> serde::de::Visitor<'de> for PartialVarVisitor {
    type Value = VarOption;

    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("a recipe var option")
    }

    fn visit_map<A>(self, mut map: A) -> std::result::Result<Self::Value, A::Error>
    where
        A: serde::de::MapAccess<'de>,
    {
        let var = map.next_value::<NameAndValue<OptNameBuf>>()?;
        let mut choices = Vec::new();
        let mut at_runtime = VarPropagation::default();
        let mut at_downstream_runtime = VarPropagation::default();
        let mut at_build = VarPropagation::default();
        let mut at_downstream_build = VarPropagation::default();
        let mut when = WhenBlock::default();
        while let Some(key) = map.next_key::<Stringified>()? {
            match key.as_str() {
                "choices" => choices = map.next_value()?,
                "atRuntime" => at_runtime = map.next_value()?,
                "atDownstreamRuntime" => at_downstream_runtime = map.next_value()?,
                "atBuild" => at_build = map.next_value()?,
                "atDownstreamBuild" => at_downstream_build = map.next_value()?,
                "when" => when = map.next_value()?,
                _name => {
                    // unrecognized fields are explicitly ignored in case
                    // they were added in a newer version of spk. We assume
                    // that if the api has not been versioned then the desire
                    // is to continue working in this older version
                    #[cfg(not(test))]
                    map.next_value::<serde::de::IgnoredAny>()?;
                    // except during testing, where we don't want to hide
                    // failing tests because of ignored data
                    #[cfg(test)]
                    return Err(serde::de::Error::unknown_field(_name, &[]));
                }
            }
        }
        Ok(VarOption {
            var,
            choices,
            at_build,
            at_runtime,
            at_downstream_build,
            at_downstream_runtime,
            when,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub enum VarPropagation {
    /// The package request is not propagated to downstream environments
    Disabled,
    Enabled {
        when: WhenBlock,
    },
}

impl Default for VarPropagation {
    fn default() -> Self {
        Self::Enabled {
            when: Default::default(),
        }
    }
}

impl VarPropagation {
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }
}

impl<'de> Deserialize<'de> for VarPropagation {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct VarPropagationVisitor;

        impl<'de> serde::de::Visitor<'de> for VarPropagationVisitor {
            type Value = VarPropagation;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a boolean or mapping")
            }

            fn visit_bool<E>(self, v: bool) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match v {
                    true => Ok(VarPropagation::default()),
                    false => Ok(VarPropagation::Disabled),
                }
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                let mut when = WhenBlock::default();
                while let Some(key) = map.next_key::<Stringified>()? {
                    match key.as_str() {
                        "when" => when = map.next_value()?,
                        _name => {
                            // unrecognized fields are explicitly ignored in case
                            // they were added in a newer version of spk. We assume
                            // that if the api has not been versioned then the desire
                            // is to continue working in this older version
                            #[cfg(not(test))]
                            map.next_value::<serde::de::IgnoredAny>()?;
                            // except during testing, where we don't want to hide
                            // failing tests because of ignored data
                            #[cfg(test)]
                            return Err(serde::de::Error::unknown_field(_name, &[]));
                        }
                    }
                }
                Ok(VarPropagation::Enabled { when })
            }
        }

        deserializer.deserialize_any(VarPropagationVisitor)
    }
}

impl serde::Serialize for VarPropagation {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Self::Disabled => serializer.serialize_bool(false),
            Self::Enabled { when } => {
                use serde::ser::SerializeMap;
                let mut map = serializer.serialize_map(Some(3))?;
                if !when.is_always() {
                    map.serialize_entry("when", when)?;
                }
                map.end()
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PkgOption {
    pub pkg: RangeIdent,
    #[serde(default, skip_serializing_if = "PkgPropagation::is_default")]
    pub at_build: PkgPropagation,
    #[serde(default, skip_serializing_if = "PkgPropagation::is_default")]
    pub at_runtime: PkgPropagation,
    #[serde(default, skip_serializing_if = "PkgPropagation::is_default")]
    pub at_downstream_build: PkgPropagation,
    #[serde(default, skip_serializing_if = "PkgPropagation::is_default")]
    pub at_downstream_runtime: PkgPropagation,
    #[serde(skip_serializing_if = "WhenBlock::is_always")]
    pub when: WhenBlock,
}

/// This visitor is partial because it expects that the first
/// 'pkg' field has already been partially read. That is, the
/// key has been seen and validated, and so this visitor will
/// continue by reading the value of that field. In all other
/// cases, this will cause the deserializer to fail, and so
/// this type should not be used outside of the specific use
/// case of this module.
struct PartialPkgVisitor;

impl<'de> serde::de::Visitor<'de> for PartialPkgVisitor {
    type Value = PkgOption;

    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("a recipe pkg option")
    }

    fn visit_map<A>(self, mut map: A) -> std::result::Result<Self::Value, A::Error>
    where
        A: serde::de::MapAccess<'de>,
    {
        let pkg = map.next_value()?;
        let mut at_runtime = PkgPropagation::default();
        let mut at_build = PkgPropagation::default();
        let mut at_downstream_build = PkgPropagation::default();
        let mut at_downstream_runtime = PkgPropagation::default();
        let mut when = WhenBlock::default();
        while let Some(key) = map.next_key::<Stringified>()? {
            match key.as_str() {
                "atBuild" => at_build = map.next_value()?,
                "atRuntime" => at_runtime = map.next_value()?,
                "atDownstreamBuild" => at_downstream_build = map.next_value()?,
                "atDownstreamRuntime" => at_downstream_runtime = map.next_value()?,
                "when" => when = map.next_value()?,
                _name => {
                    // unrecognized fields are explicitly ignored in case
                    // they were added in a newer version of spk. We assume
                    // that if the api has not been versioned then the desire
                    // is to continue working in this older version
                    #[cfg(not(test))]
                    map.next_value::<serde::de::IgnoredAny>()?;
                    // except during testing, where we don't want to hide
                    // failing tests because of ignored data
                    #[cfg(test)]
                    return Err(serde::de::Error::unknown_field(_name, &[]));
                }
            }
        }
        Ok(PkgOption {
            pkg,
            at_build,
            at_runtime,
            at_downstream_build,
            at_downstream_runtime,
            when,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub enum PkgPropagation {
    /// The package request is not propagated to downstream environments
    Disabled,
    Enabled {
        version: Option<String>,
        components: BTreeSet<Component>,
        when: WhenBlock,
    },
}

impl Default for PkgPropagation {
    fn default() -> Self {
        Self::Enabled {
            version: Some(String::from("Binary")),
            components: Default::default(),
            when: Default::default(),
        }
    }
}

impl PkgPropagation {
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }
}

impl<'de> Deserialize<'de> for PkgPropagation {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct PkgPropagationVisitor;

        impl<'de> serde::de::Visitor<'de> for PkgPropagationVisitor {
            type Value = PkgPropagation;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a boolean or mapping")
            }

            fn visit_bool<E>(self, v: bool) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match v {
                    true => Ok(PkgPropagation::default()),
                    false => Ok(PkgPropagation::Disabled),
                }
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                let mut version = None;
                let mut components = BTreeSet::default();
                let mut when = WhenBlock::default();
                while let Some(key) = map.next_key::<Stringified>()? {
                    match key.as_str() {
                        "version" => version = Some(map.next_value()?),
                        "components" => components = map.next_value()?,
                        "when" => when = map.next_value()?,
                        _name => {
                            // unrecognized fields are explicitly ignored in case
                            // they were added in a newer version of spk. We assume
                            // that if the api has not been versioned then the desire
                            // is to continue working in this older version
                            #[cfg(not(test))]
                            map.next_value::<serde::de::IgnoredAny>()?;
                            // except during testing, where we don't want to hide
                            // failing tests because of ignored data
                            #[cfg(test)]
                            return Err(serde::de::Error::unknown_field(_name, &[]));
                        }
                    }
                }
                Ok(PkgPropagation::Enabled {
                    version,
                    components,
                    when,
                })
            }
        }

        deserializer.deserialize_any(PkgPropagationVisitor)
    }
}

impl serde::Serialize for PkgPropagation {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Self::Disabled => serializer.serialize_bool(false),
            Self::Enabled {
                version,
                components,
                when,
            } => {
                use serde::ser::SerializeMap;
                let mut map = serializer.serialize_map(Some(3))?;
                if !version.is_none() {
                    map.serialize_entry("version", version)?;
                }
                if !components.is_empty() {
                    map.serialize_entry("components", components)?;
                }
                if !when.is_always() {
                    map.serialize_entry("when", when)?;
                }
                map.end()
            }
        }
    }
}

#[derive(Clone, Default, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub enum WhenBlock {
    #[default]
    Always,
    Sometimes {
        conditions: Vec<WhenCondition>,
    },
}

impl WhenBlock {
    pub fn is_always(&self) -> bool {
        matches!(self, Self::Always)
    }
}

impl<'de> Deserialize<'de> for WhenBlock {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct WhenConditionVisitor;

        impl<'de> serde::de::Visitor<'de> for WhenConditionVisitor {
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
        }

        deserializer.deserialize_any(WhenConditionVisitor)
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
    Pkg { pkg: RangeIdent },
    Var(VarRequest),
}

impl<'de> Deserialize<'de> for WhenCondition {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
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
                        "pkg" => result.replace(WhenCondition::Pkg {
                            pkg: map.next_value()?,
                        }),
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
                            "multiple conditions found in a single map, was this meant to be a list?"
                        ));
                    }
                }
                result.ok_or_else(|| serde::de::Error::missing_field("pkg\" or \"var"))
            }
        }

        deserializer.deserialize_any(WhenConditionVisitor)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd, Serialize)]
#[serde(untagged)]
pub enum RecipeOption {
    Var(VarOption),
    Pkg(PkgOption),
}

impl<'de> Deserialize<'de> for RecipeOption {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        /// This visitor determines the type of option
        /// by requiring that the var or pkg field be defined
        /// before any other. Although this is counter to the
        /// idea of maps, it favours consistency and error messaging
        /// for users maintaining hand-written spec files.
        #[derive(Default)]
        struct RecipeOptionVisitor;

        impl<'de> serde::de::Visitor<'de> for RecipeOptionVisitor {
            type Value = RecipeOption;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a recipe option")
            }

            fn visit_map<A>(self, mut map: A) -> std::result::Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                let first_key = map
                    .next_key::<Stringified>()?
                    .ok_or_else(|| serde::de::Error::missing_field("var\" or \"pkg"))?;
                match first_key.as_str() {
                    "pkg" => {
                        Ok(Self::Value::Pkg(PartialPkgVisitor.visit_map(map)?))
                    },
                    "var" => {
                        Ok(Self::Value::Var(PartialVarVisitor.visit_map(map)?))
                    },
                        other => {
                            Err(serde::de::Error::custom(format!("An option must declare either the 'var' or 'pkg' field before any other, found '{other}'")))
                        }
                    }
            }
        }

        deserializer.deserialize_map(RecipeOptionVisitor)
    }
}

// Package options define the set of dependencies and inputs variables
// to the package build process.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq, Serialize)]
#[serde(transparent)]
pub struct RecipeOptionList(Vec<RecipeOption>);

impl RecipeOptionList {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl std::ops::Deref for RecipeOptionList {
    type Target = Vec<RecipeOption>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for RecipeOptionList {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<'de> Deserialize<'de> for RecipeOptionList {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct OptionListVisitor;

        impl<'de> serde::de::Visitor<'de> for OptionListVisitor {
            type Value = RecipeOptionList;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a list of package options")
            }

            fn visit_unit<E>(self) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(RecipeOptionList::default())
            }

            fn visit_seq<A>(self, mut seq: A) -> std::result::Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let size_hint = seq.size_hint().unwrap_or(0);
                let mut options = Vec::with_capacity(size_hint);
                while let Some(option) = seq.next_element()? {
                    options.push(option)
                }
                Ok(RecipeOptionList(options))
            }
        }

        deserializer.deserialize_seq(OptionListVisitor)
    }
}
