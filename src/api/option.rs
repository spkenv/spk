// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::{convert::TryFrom, str::FromStr};

use indexmap::set::IndexSet;
use serde::{Deserialize, Serialize};

use super::{
    CompatRule, Compatibility, InclusionPolicy, PkgName, PkgRequest, PreReleasePolicy, Ranged,
    Request, RequestedBy, VarRequest, VersionRange,
};
use crate::{Error, Result};

#[cfg(test)]
#[path = "./option_test.rs"]
mod option_test;

/// Defines the way in which a build option in inherited by downstream packages.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord)]
pub enum Inheritance {
    // the default value, not inherited by downstream packages unless redefined
    Weak,
    // inherited by downstream packages as a build option only
    StrongForBuildOnly,
    // inherited by downstream packages as both build options and install requirement
    Strong,
}

impl std::fmt::Display for Inheritance {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_fmt(format_args!("{:?}", self))
    }
}

impl std::str::FromStr for Inheritance {
    type Err = crate::Error;
    fn from_str(value: &str) -> crate::Result<Self> {
        Ok(serde_yaml::from_str(value)?)
    }
}

impl Default for Inheritance {
    fn default() -> Self {
        Self::Weak
    }
}

impl Inheritance {
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }
}

/// An option that can be provided to provided to the package build process
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(untagged)]
pub enum Opt {
    Pkg(PkgOpt),
    Var(VarOpt),
}

impl Opt {
    pub fn name(&self) -> &str {
        match self {
            Self::Pkg(opt) => &opt.pkg,
            Self::Var(opt) => &opt.var,
        }
    }

    pub fn namespaced_name<S: AsRef<str>>(&self, pkg: S) -> String {
        match self {
            Self::Pkg(opt) => opt.namespaced_name(pkg.as_ref()),
            Self::Var(opt) => opt.namespaced_name(pkg.as_ref()),
        }
    }

    pub fn validate(&self, value: Option<&str>) -> Compatibility {
        match self {
            Self::Pkg(opt) => opt.validate(value),
            Self::Var(opt) => opt.validate(value),
        }
    }

    /// Assign a value to this option.
    ///
    /// Once a value is assigned, it overrides any 'given' value on future access.
    pub fn set_value(&mut self, value: String) -> Result<()> {
        match self {
            Self::Pkg(opt) => opt.set_value(value),
            Self::Var(opt) => opt.set_value(value),
        }
    }

    /// Return the current value of this option, if set.
    ///
    /// Given is only returned if the option is not currently set to something else.
    pub fn get_value(&self, given: Option<&str>) -> String {
        let value = match self {
            Self::Pkg(opt) => opt.get_value(given),
            Self::Var(opt) => opt.get_value(given),
        };
        match (value, given) {
            (Some(v), _) => v,
            (_, Some(v)) => v.to_string(),
            (None, None) => "".to_string(),
        }
    }
}

impl TryFrom<Request> for Opt {
    type Error = Error;
    /// Create a build option from the given request."""
    fn try_from(request: Request) -> Result<Opt> {
        match request {
            Request::Pkg(request) => {
                let default = request
                    .pkg
                    .to_string()
                    .chars()
                    .skip(request.pkg.name.len())
                    .skip(1)
                    .collect();
                Ok(Opt::Pkg(PkgOpt {
                    pkg: request.pkg.name.clone(),
                    default,
                    prerelease_policy: request.prerelease_policy,
                    value: None,
                    required_compat: request.required_compat,
                }))
            }
            Request::Var(_) => Err(Error::String(format!(
                "Cannot convert {:?} to option",
                request
            ))),
        }
    }
}

impl<'de> Deserialize<'de> for Opt {
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
        if mapping.get(&Value::String("var".to_string())).is_some() {
            Ok(Opt::Var(
                VarOpt::deserialize(Value::Mapping(mapping))
                    .map_err(|e| serde::de::Error::custom(format!("{:?}", e)))?,
            ))
        } else if mapping.get(&Value::String("pkg".to_string())).is_some() {
            Ok(Opt::Pkg(
                PkgOpt::deserialize(Value::Mapping(mapping))
                    .map_err(|e| serde::de::Error::custom(format!("{:?}", e)))?,
            ))
        } else {
            Err(serde::de::Error::custom(
                "failed to determine option type: must have one of 'var' or 'pkg' fields",
            ))
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VarOpt {
    pub var: String,
    pub default: String,
    pub choices: IndexSet<String>,
    pub inheritance: Inheritance,
    value: Option<String>,
}

// This is safe to allow because choices is IndexSet and has
// deterministic iteration order.
#[allow(clippy::derive_hash_xor_eq)]
impl std::hash::Hash for VarOpt {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.var.hash(state);
        self.default.hash(state);
        for (i, choice) in self.choices.iter().enumerate() {
            i.hash(state);

            choice.hash(state);
        }
        self.inheritance.hash(state);
        self.value.hash(state)
    }
}

impl Ord for VarOpt {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match self.var.cmp(&other.var) {
            core::cmp::Ordering::Equal => {}
            ord => return ord,
        }
        match self.default.cmp(&other.default) {
            core::cmp::Ordering::Equal => {}
            ord => return ord,
        }
        match self.choices.iter().cmp(other.choices.iter()) {
            core::cmp::Ordering::Equal => {}
            ord => return ord,
        }
        match self.inheritance.cmp(&other.inheritance) {
            core::cmp::Ordering::Equal => {}
            ord => return ord,
        }
        self.value.cmp(&other.value)
    }
}

impl PartialOrd for VarOpt {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        match self.var.partial_cmp(&other.var) {
            Some(core::cmp::Ordering::Equal) => {}
            ord => return ord,
        }
        match self.default.partial_cmp(&other.default) {
            Some(core::cmp::Ordering::Equal) => {}
            ord => return ord,
        }
        match self.choices.iter().partial_cmp(other.choices.iter()) {
            Some(core::cmp::Ordering::Equal) => {}
            ord => return ord,
        }
        match self.inheritance.partial_cmp(&other.inheritance) {
            Some(core::cmp::Ordering::Equal) => {}
            ord => return ord,
        }
        self.value.partial_cmp(&other.value)
    }
}

impl VarOpt {
    pub fn new<S: AsRef<str>>(var: S) -> Self {
        Self {
            var: var.as_ref().to_string(),
            default: String::default(),
            choices: IndexSet::default(),
            inheritance: Inheritance::default(),
            value: None,
        }
    }

    pub fn namespaced_name(&self, pkg: &str) -> String {
        if self.var.contains('.') {
            self.var.clone()
        } else {
            format!("{}.{}", pkg, self.var)
        }
    }

    pub fn get_value(&self, given: Option<&str>) -> Option<String> {
        if let Some(v) = &self.value {
            if !v.is_empty() {
                return Some(v.clone());
            }
        }
        if let Some(v) = given {
            Some(v.to_string())
        } else if !self.default.is_empty() {
            Some(self.default.clone())
        } else {
            None
        }
    }

    pub fn set_value(&mut self, value: String) -> Result<()> {
        if !self.choices.is_empty() && !value.is_empty() && !self.choices.contains(&value) {
            return Err(Error::String(format!(
                "Invalid value '{}' for option '{}', must be one of {:?}",
                value, self.var, self.choices
            )));
        }
        self.value = Some(value);
        Ok(())
    }

    pub fn validate(&self, value: Option<&str>) -> Compatibility {
        if value.is_none() && self.value.is_some() {
            return self.validate(self.value.as_deref());
        }
        let assigned = self.value.as_deref();
        match (value, assigned) {
            (None, Some(_)) => Compatibility::Compatible,
            (Some(value), Some(assigned)) => {
                if value == assigned {
                    Compatibility::Compatible
                } else {
                    Compatibility::Incompatible(format!(
                        "incompatible option, wanted '{}', got '{}'",
                        assigned, value
                    ))
                }
            }
            (Some(value), _) => {
                if !self.choices.is_empty() && !self.choices.contains(value) {
                    return Compatibility::Incompatible(format!(
                        "invalid value '{}', must be one of {:?}",
                        value, self.choices
                    ));
                } else {
                    Compatibility::Compatible
                }
            }
            (_, None) => Compatibility::Compatible,
        }
    }

    pub fn to_request(&self, given_value: Option<&str>) -> VarRequest {
        let value = self.get_value(given_value).unwrap_or_default();
        VarRequest {
            var: self.var.clone(),
            value,
            pin: false,
        }
    }
}

#[derive(Serialize, Deserialize)]
struct VarOptSchema {
    var: String,
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "strings_from_scalars"
    )]
    choices: Vec<String>,
    #[serde(default, skip_serializing_if = "Inheritance::is_default")]
    inheritance: Inheritance,
    #[serde(
        default,
        rename = "static",
        skip_serializing_if = "String::is_empty",
        deserialize_with = "super::option_map::string_from_scalar"
    )]
    value: String,
    // the default field can be loaded for legacy compatibility but is deprecated
    #[serde(
        default,
        skip_serializing,
        deserialize_with = "optional_string_from_scalar"
    )]
    default: Option<String>,
}

impl Serialize for VarOpt {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        let mut out = VarOptSchema {
            var: self.var.clone(),
            choices: self.choices.iter().map(String::to_owned).collect(),
            inheritance: self.inheritance,
            value: self.value.clone().unwrap_or_default(),
            default: None,
        };
        if !self.default.is_empty() {
            out.var = format!("{}/{}", self.var, self.default);
        }

        out.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for VarOpt {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let data = VarOptSchema::deserialize(deserializer)?;
        let mut out = VarOpt {
            var: data.var.clone(),
            default: "".to_string(),
            choices: data.choices.iter().map(String::to_owned).collect(),
            inheritance: data.inheritance,
            value: None,
        };
        if let Some(default) = data.default {
            // the default field is deprecated, but we support it for existing packages
            out.default = default;
        } else {
            let mut split = data.var.split('/');
            out.var = split.next().unwrap().to_string();
            out.default = split.collect::<Vec<_>>().join("");
        }

        if !data.value.is_empty() {
            out.value = Some(data.value.to_owned());
        }
        Ok(out)
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PkgOpt {
    pub pkg: PkgName,
    pub default: String,
    pub prerelease_policy: PreReleasePolicy,
    pub required_compat: Option<CompatRule>,
    value: Option<String>,
}

impl PkgOpt {
    pub fn new(name: PkgName) -> Result<Self> {
        Ok(Self {
            pkg: name,
            default: String::default(),
            prerelease_policy: PreReleasePolicy::default(),
            value: None,
            required_compat: None,
        })
    }

    pub fn get_value(&self, given: Option<&str>) -> Option<String> {
        if let Some(v) = &self.value {
            Some(v.clone())
        } else if let Some(v) = given {
            Some(v.to_string())
        } else {
            Some(self.default.clone())
        }
    }

    pub fn set_value(&mut self, value: String) -> Result<()> {
        if let Err(err) = VersionRange::from_str(&value) {
            return Err(Error::wrap(
                format!(
                    "Invalid value '{}' for option '{}', not a valid package request",
                    value, self.pkg
                ),
                err,
            ));
        }
        self.value = Some(value);
        Ok(())
    }

    pub fn namespaced_name(&self, pkg: &str) -> String {
        format!("{}.{}", pkg, self.pkg)
    }

    pub fn validate(&self, value: Option<&str>) -> Compatibility {
        let value = value.unwrap_or_default();

        // skip any default that might exist since
        // that does not represent a definitive range
        let base = match &self.value {
            None => return Compatibility::Compatible,
            Some(v) => v,
        };
        let base_range = match VersionRange::from_str(base) {
            Err(err) => {
                return Compatibility::Incompatible(format!(
                    "Invalid value '{}' for option '{}', not a valid package request: {}",
                    base, self.pkg, err
                ))
            }
            Ok(r) => r,
        };
        match VersionRange::from_str(value) {
            Err(err) => Compatibility::Incompatible(format!(
                "Invalid value '{}' for option '{}', not a valid package request: {}",
                value, self.pkg, err
            )),
            Ok(value_range) => value_range.contains(&base_range),
        }
    }

    pub fn to_request(
        &self,
        given_value: Option<String>,
        requester: RequestedBy,
    ) -> Result<PkgRequest> {
        let value = self.get_value(given_value.as_deref()).unwrap_or_default();
        let pkg = super::RangeIdent {
            name: self.pkg.clone(),
            version: value.parse()?,
            components: Default::default(),
            build: None,
        };

        let request = PkgRequest::new(pkg, requester)
            .with_prerelease(self.prerelease_policy)
            .with_inclusion(InclusionPolicy::default())
            .with_pin(None)
            .with_compat(self.required_compat);
        Ok(request)
    }
}

#[derive(Serialize, Deserialize)]
struct PkgOptSchema {
    pkg: String,
    #[serde(
        default,
        rename = "prereleasePolicy",
        skip_serializing_if = "PreReleasePolicy::is_default"
    )]
    prerelease_policy: PreReleasePolicy,
    #[serde(
        default,
        rename = "static",
        skip_serializing_if = "String::is_empty",
        deserialize_with = "super::option_map::string_from_scalar"
    )]
    value: String,
    // the default field can be loaded for legacy compatibility but is deprecated
    #[serde(
        default,
        skip_serializing,
        deserialize_with = "optional_string_from_scalar"
    )]
    default: Option<String>,
}

impl Serialize for PkgOpt {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        let mut out = PkgOptSchema {
            pkg: self.pkg.to_string(),
            prerelease_policy: self.prerelease_policy,
            value: self.value.clone().unwrap_or_default(),
            default: None,
        };
        if !self.default.is_empty() {
            out.pkg = format!("{}/{}", self.pkg, self.default);
        }

        out.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for PkgOpt {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let data = PkgOptSchema::deserialize(deserializer)?;

        let (pkg, default) = if let Some(default) = data.default {
            // the default field is deprecated, but we support it for existing packages
            (data.pkg.parse().map_err(serde::de::Error::custom)?, default)
        } else {
            let mut split = data.pkg.split('/');
            (
                split
                    .next()
                    .unwrap()
                    .parse()
                    .map_err(serde::de::Error::custom)?,
                split.collect::<Vec<_>>().join(""),
            )
        };

        let mut out = PkgOpt {
            pkg,
            default,
            prerelease_policy: data.prerelease_policy,
            value: None,
            required_compat: None,
        };

        if let Compatibility::Incompatible(err) = out.validate(Some(&out.default)) {
            return Err(serde::de::Error::custom(err));
        }

        if !data.value.is_empty() {
            out.value = Some(data.value.to_owned());
            if let Compatibility::Incompatible(err) = out.validate(Some(&data.value)) {
                return Err(serde::de::Error::custom(err));
            }
        }
        Ok(out)
    }
}

/// Deserialize any reasonable scalar option (int, float, str, null) to an Option<String> value
pub(crate) fn optional_string_from_scalar<'de, D>(
    deserializer: D,
) -> std::result::Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde_yaml::Value;
    let value = Value::deserialize(deserializer)?;
    match value {
        Value::Bool(b) => Ok(Some(b.to_string())),
        Value::Number(n) => Ok(Some(n.to_string())),
        Value::String(s) => Ok(Some(s)),
        Value::Null => Ok(None),
        _ => Err(serde::de::Error::custom("expected scalar value")),
    }
}

/// Deserialize a list of any reasonable scalar option (int, float, str) to an Vec<String> value
pub(crate) fn strings_from_scalars<'de, D>(
    deserializer: D,
) -> std::result::Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde_yaml::Value;
    let value = Value::deserialize(deserializer)?;
    match value {
        Value::Sequence(b) => b
            .into_iter()
            .map(super::option_map::string_from_scalar)
            .collect::<serde_yaml::Result<Vec<String>>>()
            .map_err(|err| serde::de::Error::custom(format!("expected list of scalars: {}", err))),
        _ => Err(serde::de::Error::custom("expected list of scalars")),
    }
}
