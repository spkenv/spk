// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::BTreeSet;
use std::marker::PhantomData;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use spk_schema_foundation::ident_component::Component;
use spk_schema_foundation::name::{OptName, OptNameBuf};
use spk_schema_foundation::option_map::{OptionMap, Stringified};
use spk_schema_foundation::spec_ops::{HasVersion, Named};
use spk_schema_foundation::version::Compatibility;
use spk_schema_foundation::version_range::{Ranged, VersionRange};
use spk_schema_ident::{NameAndValue, PkgRequest, RangeIdent, Request, Satisfy, VarRequest};

use super::{ConditionOutcome, WhenBlock};
use crate::v1::WhenCondition;
use crate::{BuildEnv, BuildEnvMember};

#[cfg(test)]
#[path = "./recipe_option_test.rs"]
mod recipe_option_test;

#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd, Serialize)]
#[serde(untagged)]
pub enum RecipeOption {
    Var(Box<VarOption>),
    Pkg(Box<PkgOption>),
}

impl RecipeOption {
    pub fn name(&self) -> &OptName {
        match self {
            Self::Pkg(p) => p.pkg.name.as_opt_name(),
            Self::Var(v) => v.var.name().as_ref(),
        }
    }

    pub fn value<'a: 'out, 'b: 'out, 'out>(&'a self, given: Option<&'b String>) -> Option<String> {
        match self {
            Self::Pkg(p) => p.value(given),
            Self::Var(v) => v.var.value(given).cloned(),
        }
    }

    pub fn validate(&self, value: &String) -> Compatibility {
        match self {
            Self::Pkg(p) => p.validate(value),
            Self::Var(v) => v.validate(value),
        }
    }

    /// Create a solver request from this option
    pub fn to_request<V>(&self, variant: V) -> Option<Request>
    where
        V: crate::Variant,
    {
        match self {
            Self::Pkg(p) => Some(Request::Pkg(p.to_request(variant))),
            Self::Var(v) => v.to_request(variant).map(Request::Var),
        }
    }

    pub fn is_pkg(&self) -> bool {
        matches!(self, Self::Pkg(_))
    }

    pub fn as_pkg(&self) -> Option<&PkgOption> {
        match self {
            Self::Pkg(o) => Some(o),
            _ => None,
        }
    }

    pub fn into_pkg(self) -> Option<Box<PkgOption>> {
        match self {
            Self::Pkg(o) => Some(o),
            _ => None,
        }
    }

    pub fn is_var(&self) -> bool {
        matches!(self, Self::Var(_))
    }

    pub fn as_var(&self) -> Option<&VarOption> {
        match self {
            Self::Var(o) => Some(o),
            _ => None,
        }
    }

    pub fn into_var(self) -> Option<Box<VarOption>> {
        match self {
            Self::Var(o) => Some(o),
            _ => None,
        }
    }

    /// Determine if this option is enabled given the resolved
    /// build environment. If not, the returned compatibility will
    /// denote a reason why it has been disabled.
    pub fn check_is_active_at_build(&self, options: &OptionMap) -> ConditionOutcome {
        match self {
            Self::Pkg(p) => p.at_build.check_is_active(options),
            Self::Var(v) => v.at_build.check_is_active(options),
        }
    }

    /// Determine if this option is enabled given the resolved
    /// build environment. If not, the returned compatibility will
    /// denote a reason why it has been disabled.
    pub fn check_is_active_at_runtime<E>(&self, build_env: E) -> ConditionOutcome
    where
        E: BuildEnv,
        E::Package: Satisfy<PkgRequest> + Named + HasVersion,
    {
        match self {
            Self::Pkg(p) => p.at_runtime.check_is_active(&p.pkg, build_env),
            Self::Var(v) => v.at_runtime.check_is_active(build_env),
        }
    }

    /// Determine if this option is enabled given the resolved
    /// build environment. If not, the returned compatibility will
    /// denote a reason why it has been disabled.
    pub fn check_is_active_at_downstream<E>(&self, build_env: E) -> ConditionOutcome
    where
        E: BuildEnv,
        E::Package: Satisfy<PkgRequest> + Named + HasVersion,
    {
        match self {
            Self::Pkg(p) => p.at_downstream.check_is_active(&p.pkg, build_env),
            Self::Var(v) => v.at_downstream.check_is_active(build_env),
        }
    }
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
                        Ok(Self::Value::Pkg(PartialPkgVisitor.visit_map(map)?.into()))
                    },
                    "var" => {
                        Ok(Self::Value::Var(PartialVarVisitor.visit_map(map)?.into()))
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

#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VarOption {
    pub var: NameAndValue<OptNameBuf>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub choices: Vec<String>,
    #[serde(default, skip_serializing_if = "BuildCondition::is_default")]
    pub at_build: BuildCondition,
    #[serde(default, skip_serializing_if = "VarPropagation::is_default")]
    pub at_runtime: VarPropagation,
    #[serde(
        default = "VarPropagation::disabled",
        skip_serializing_if = "VarPropagation::is_disabled"
    )]
    pub at_downstream: VarPropagation,
}

impl VarOption {
    pub fn validate(&self, value: &String) -> Compatibility {
        if !self.choices.is_empty() && !self.choices.contains(value) {
            return Compatibility::incompatible(format!(
                "invalid option value '{}={}', must be one of {:?}",
                self.var.name(),
                value,
                self.choices
            ));
        }
        Compatibility::Compatible
    }

    /// Create a solver request from this option, if appropriate
    pub fn to_request<V>(&self, variant: V) -> Option<VarRequest>
    where
        V: crate::Variant,
    {
        let variant_options = variant.options();
        // TODO: account for inheritance and the possible unqualified var name
        let given = variant_options.get(self.var.name());
        self.var.value(given).map(|value| VarRequest {
            var: self.var.name().clone(),
            pin: false,
            value: value.to_owned(),
        })
    }
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
        let mut at_runtime = Option::<VarPropagation>::None;
        let mut at_downstream = Option::<VarPropagation>::None;
        let mut at_build = Option::<BuildCondition>::None;
        let mut when = Option::<VarPropagation>::None;
        while let Some(key) = map.next_key::<Stringified>()? {
            match key.as_str() {
                "choices" => choices = map.next_value()?,
                "atBuild" => at_build = Some(map.next_value()?),
                "atRuntime" => at_runtime = Some(map.next_value()?),
                "atDownstream" => at_downstream = Some(map.next_value()?),
                "when" => {
                    when = Some(VarPropagation::Enabled {
                        when: map.next_value()?,
                    })
                }
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
            at_runtime: at_runtime.or_else(|| when.clone()).unwrap_or_default(),
            at_downstream: at_downstream.or_else(|| when.clone()).unwrap_or(VarPropagation::Disabled),
            at_build: at_build
            .or_else(|| {
                let Some(when) = when else {
                    return None;
                };
                match when.try_into() {
                    Ok(mapped) => Some(mapped),
                    Err(err) => {
                        tracing::warn!("Shared 'when' condition is invalid at build-time");
                        tracing::warn!(" > because: {err}");
                        tracing::warn!(" > to remove this message, add 'atBuild: true' to the option for 'var: {}'", var.name());
                        None
                    }
                }}).unwrap_or_default(),
                var,
                choices,
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

    pub fn is_disabled(&self) -> bool {
        matches!(self, Self::Disabled)
    }

    pub fn disabled() -> Self {
        Self::Disabled
    }

    pub fn check_is_active<E>(&self, build_env: E) -> ConditionOutcome
    where
        E: BuildEnv,
        E::Package: Satisfy<PkgRequest> + Named,
    {
        match self {
            Self::Disabled => ConditionOutcome::disabled("disabled"),
            Self::Enabled { when } => when.check_is_active(build_env),
        }
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
#[cfg_attr(test, serde(deny_unknown_fields))]
#[serde(rename_all = "camelCase")]
pub struct PkgOption {
    pub pkg: RangeIdent,
    #[serde(default, skip_serializing_if = "BuildCondition::is_default")]
    pub at_build: BuildCondition,
    #[serde(default, skip_serializing_if = "PkgPropagation::is_default")]
    pub at_runtime: PkgPropagation,
    #[serde(default, skip_serializing_if = "PkgPropagation::is_default")]
    pub at_downstream: PkgPropagation,

    // included specifically to catch the common error of putting
    // this field on pkg options but having it be ignored
    #[serde(default, skip_serializing, deserialize_with = "no_downstream_build")]
    at_downstream_build: PhantomData<()>,
}

fn no_downstream_build<'de, D>(_deserializer: D) -> std::result::Result<PhantomData<()>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Err(no_downstream_build_error())
}

fn no_downstream_build_error<E: serde::de::Error>() -> E {
    serde::de::Error::custom(
        "The 'atDownstreamBuild' field does not really make \
        sense for pkg options. Instead, use 'atDownstream' \
        with 'components: [build]' and/or any other relevant \
        components that would require this dependency when used",
    )
}

impl PkgOption {
    pub fn value(&self, given: Option<&String>) -> Option<String> {
        if self.pkg.version.is_empty() && self.pkg.build.is_none() {
            given.cloned()
        } else {
            Some(self.pkg.range_value())
        }
    }

    pub fn validate(&self, value: &String) -> Compatibility {
        match VersionRange::from_str(value) {
            Err(err) => Compatibility::incompatible(format!(
                "Invalid value '{}' for option '{}', not a valid package request: {}",
                value, self.pkg, err
            )),
            Ok(value_range) => value_range.intersects(&self.pkg.version),
        }
    }

    /// Create a solver request from this option
    pub fn to_request<V>(&self, variant: V) -> PkgRequest
    where
        V: crate::Variant,
    {
        let variant_options = variant.options();
        let pkg = variant_options
            .get(self.pkg.name.as_opt_name())
            .map(|v| format!("{}/{v}", self.pkg.name))
            // TODO: warn about the invalid value?
            .and_then(|v| RangeIdent::from_str(&v).ok())
            .unwrap_or_else(|| self.pkg.clone());
        PkgRequest {
            pkg,
            prerelease_policy: Default::default(),
            inclusion_policy: Default::default(),
            pin: Default::default(),
            required_compat: Default::default(),
            requested_by: Default::default(),
        }
    }
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
        let mut at_runtime = Option::<PkgPropagation>::None;
        let mut at_build = Option::<BuildCondition>::None;
        let mut at_downstream = Option::<PkgPropagation>::None;
        let mut when = Option::<PkgPropagation>::None;
        while let Some(key) = map.next_key::<Stringified>()? {
            match key.as_str() {
                "atBuild" => at_build = Some(map.next_value()?),
                "atRuntime" => at_runtime = Some(map.next_value()?),
                "atDownstreamBuild" => return Err(no_downstream_build_error()),
                "atDownstream" => at_downstream = Some(map.next_value()?),
                "when" => {
                    when = Some(PkgPropagation::Enabled {
                        version: None,
                        components: Default::default(),
                        when: map.next_value()?,
                    })
                }
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
            at_runtime: at_runtime.or_else(|| when.clone()).unwrap_or_default(),
            at_downstream: at_downstream.or_else(|| when.clone()).unwrap_or(PkgPropagation::Disabled),
            at_build: at_build
            .or_else(|| {
                let Some(when) = when else {
                    return None;
                };
                match when.try_into() {
                    Ok(mapped) => Some(mapped),
                    Err(err) => {
                        tracing::warn!("Shared 'when' condition is invalid at build-time");
                        tracing::warn!(" > because: {err}");
                        tracing::warn!(" > to remove this message, add 'atBuild: true' to the option for 'pkg: {pkg}'");
                        None
                    }
                }}).unwrap_or_default(),
                at_downstream_build: PhantomData,
                pkg,
            })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub enum PkgPropagation {
    /// The package request is not propagated to this environments
    Disabled,
    Enabled {
        /// An alternate version to use when resolving this package
        version: Option<String>,
        /// An alternate set of packages to resolve for this package
        components: BTreeSet<Component>,
        /// Conditions for which to resolve this package
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

    pub fn check_is_active<E>(&self, pkg: &RangeIdent, build_env: E) -> ConditionOutcome
    where
        E: BuildEnv,
        E::Package: Satisfy<PkgRequest> + Named + HasVersion,
    {
        match self {
            Self::Disabled => ConditionOutcome::disabled("disabled"),
            Self::Enabled { when, .. } => {
                if let Some(member) = build_env.get_member(pkg.name()) {
                    let resolved_components = member.used_components();
                    if !pkg.components.is_empty()
                        && !pkg
                            .components
                            .iter()
                            .any(|c| resolved_components.contains(c))
                    {
                        return ConditionOutcome::disabled(format!(
                            "None of the specified components are present: wanted {:?}, found {:?}",
                            pkg.components, resolved_components
                        ));
                    }
                    let resolved_package = member.package();
                    let compat = pkg.version.is_applicable(resolved_package.version());
                    if let Compatibility::Incompatible { reason } = compat {
                        return ConditionOutcome::disabled(reason);
                    }
                }
                when.check_is_active(build_env)
            }
        }
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

#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub enum BuildCondition {
    /// The request is not active in build environments
    Disabled,
    Enabled {
        when: WhenBlock<VarRequest>,
    },
}

impl Default for BuildCondition {
    fn default() -> Self {
        Self::Enabled {
            when: WhenBlock::Always,
        }
    }
}

impl BuildCondition {
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }

    pub fn check_is_active(&self, options: &OptionMap) -> ConditionOutcome {
        match self {
            Self::Disabled => ConditionOutcome::disabled("disabled"),
            Self::Enabled { when } => when.check_is_active_at_build(options),
        }
    }
}

impl<'de> Deserialize<'de> for BuildCondition {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct BuildConditionVisitor;

        impl<'de> serde::de::Visitor<'de> for BuildConditionVisitor {
            type Value = BuildCondition;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a boolean or mapping")
            }

            fn visit_bool<E>(self, v: bool) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match v {
                    true => Ok(BuildCondition::default()),
                    false => Ok(BuildCondition::Disabled),
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
                Ok(BuildCondition::Enabled { when })
            }
        }

        deserializer.deserialize_any(BuildConditionVisitor)
    }
}

impl serde::Serialize for BuildCondition {
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

impl TryInto<BuildCondition> for VarPropagation {
    type Error = crate::Error;

    fn try_into(self) -> Result<BuildCondition, Self::Error> {
        match self {
            Self::Disabled => Ok(BuildCondition::Disabled),
            Self::Enabled { when } => Ok(BuildCondition::Enabled {
                when: when.try_into()?,
            }),
        }
    }
}

impl TryInto<BuildCondition> for PkgPropagation {
    type Error = crate::Error;

    fn try_into(self) -> Result<BuildCondition, Self::Error> {
        match self {
            Self::Disabled => Ok(BuildCondition::Disabled),
            Self::Enabled {
                when,
                version,
                components,
            } => {
                if version.is_some() {
                    return Err(crate::Error::String(
                        "'when.version' cannot be used at build time".to_string(),
                    ));
                }
                if !components.is_empty() {
                    return Err(crate::Error::String(
                        "'when.components' cannot be used at build time".to_string(),
                    ));
                }
                Ok(BuildCondition::Enabled {
                    when: when.try_into()?,
                })
            }
        }
    }
}

impl TryInto<WhenBlock<VarRequest>> for WhenBlock {
    type Error = crate::Error;

    fn try_into(self) -> Result<WhenBlock<VarRequest>, Self::Error> {
        match self {
            Self::Always => Ok(WhenBlock::Always),
            Self::Sometimes { conditions } => Ok(WhenBlock::Sometimes {
                conditions: conditions
                    .into_iter()
                    .map(|c| match c {
                        WhenCondition::Pkg(p) => Err(crate::Error::String(format!(
                            "pkg conditions cannot be used at build time, found {}",
                            p.pkg
                        ))),
                        WhenCondition::Var(v) => Ok(v),
                    })
                    .collect::<crate::Result<Vec<_>>>()?,
            }),
        }
    }
}