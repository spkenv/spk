// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::convert::TryFrom;
use std::str::FromStr;

use indexmap::set::IndexSet;
use serde::{Deserialize, Serialize};
use spk_schema_foundation::option_map::Stringified;
use spk_schema_ident::NameAndValue;

use crate::foundation::name::{OptName, OptNameBuf, PkgName, PkgNameBuf};
use crate::foundation::version::{CompatRule, Compatibility};
use crate::foundation::version_range::{Ranged, VersionRange};
use crate::ident::{
    parse_ident_range, InclusionPolicy, PkgRequest, PreReleasePolicy, Request, RequestedBy,
    VarRequest,
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
        serde_yaml::from_str(value).map_err(Error::InvalidInheritance)
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
    /// The name of this option with any associated namespace
    pub fn full_name(&self) -> &OptName {
        match self {
            Self::Pkg(opt) => opt.pkg.as_opt_name(),
            Self::Var(opt) => &opt.var,
        }
    }

    /// The name of this option without any associated namespace
    pub fn base_name(&self) -> &str {
        match self {
            Self::Pkg(opt) => opt.pkg.as_str(),
            Self::Var(opt) => opt.var.base_name(),
        }
    }

    /// The package namespace of this option, if any
    pub fn namespace(&self) -> Option<&PkgName> {
        match self {
            Self::Pkg(opt) => Some(&opt.pkg),
            Self::Var(opt) => opt.var.namespace(),
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

    pub fn is_pkg(&self) -> bool {
        matches!(self, Self::Pkg(_))
    }

    pub fn into_pkg(self) -> Option<PkgOpt> {
        match self {
            Self::Pkg(p) => Some(p),
            _ => None,
        }
    }

    pub fn is_var(&self) -> bool {
        matches!(self, Self::Var(_))
    }

    pub fn into_var(self) -> Option<VarOpt> {
        match self {
            Self::Var(v) => Some(v),
            _ => None,
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
        /// This visitor captures all fields that could be valid
        /// for any option, before deciding at the end which variant
        /// to actually build. We ignore any unrecognized field anyway,
        /// but additionally any field that's recognized must be valid
        /// even if it's not going to be used.
        ///
        /// The purpose of this setup is to enable more meaningful errors
        /// for invalid values that contain original source positions. In
        /// order to achieve this we must parse and validate each field with
        /// the appropriate type as they are visited - which disqualifies the
        /// existing approach to untagged enums which read all fields first
        /// and then goes back and checks them once the variant is determined
        #[derive(Default)]
        struct OptVisitor {
            // PkgOpt
            pkg: Option<PkgNameBuf>,
            prerelease_policy: Option<PreReleasePolicy>,

            // VarOpt
            var: Option<OptNameBuf>,
            choices: Option<IndexSet<String>>,
            inheritance: Option<Inheritance>,

            // Both
            default: Option<String>,
            value: Option<String>,
        }

        impl<'de> serde::de::Visitor<'de> for OptVisitor {
            type Value = Opt;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a pkg or var option")
            }

            fn visit_map<A>(mut self, mut map: A) -> std::result::Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                let check_existing_default = |v: &OptVisitor| -> std::result::Result<(), A::Error> {
                    if v.value.is_some() {
                        Err(serde::de::Error::custom("option cannot specify "))
                    } else {
                        Ok(())
                    }
                };

                while let Some(key) = map.next_key::<Stringified>()? {
                    match key.as_str() {
                        "pkg" => {
                            let NameAndValue::<PkgNameBuf>(name, value) = map.next_value()?;
                            self.pkg = Some(name);
                            if value.is_some() {
                                check_existing_default(&self)?;
                            }
                            self.default = value;
                        }
                        "prereleasePolicy" => {
                            self.prerelease_policy = Some(map.next_value::<PreReleasePolicy>()?)
                        }
                        "var" => {
                            let NameAndValue(name, value) = map.next_value()?;
                            self.var = Some(name);
                            if value.is_some() {
                                check_existing_default(&self)?;
                            }
                            self.default = value;
                        }
                        "choices" => {
                            self.choices = Some(
                                map.next_value::<Vec<Stringified>>()?
                                    .into_iter()
                                    .map(|s| s.0)
                                    .collect(),
                            )
                        }
                        "inheritance" => self.inheritance = Some(map.next_value::<Inheritance>()?),
                        "default" => {
                            check_existing_default(&self)?;
                            self.default = Some(map.next_value::<Stringified>()?.0);
                        }
                        "static" => self.value = Some(map.next_value::<Stringified>()?.0),
                        _ => {
                            // unrecognized fields are explicitly ignored in case
                            // they were added in a newer version of spk. We assume
                            // that if the api has not been versioned then the desire
                            // is to continue working in this older version
                            map.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }

                match (self.pkg, self.var) {
                    (Some(pkg), None) => Ok(Opt::Pkg(PkgOpt {
                        pkg,
                        prerelease_policy: self.prerelease_policy.unwrap_or_default(),
                        required_compat: Default::default(),
                        default: self.default.unwrap_or_default(),
                        value: self.value,
                    })),
                    (None, Some(var)) =>Ok(Opt::Var(VarOpt {
                        var,
                        choices: self.choices.unwrap_or_default(),
                        inheritance: self.inheritance.unwrap_or_default(),
                        default: self.default.unwrap_or_default(),
                        value: self.value,
                    })),
                    (Some(_), Some(_)) => Err(serde::de::Error::custom(
                        "could not determine option type, it may only contain one of the `pkg` or `var` fields"
                    )),
                    (None, None) => Err(serde::de::Error::custom(
                        "could not determine option type, it must include either a `pkg` or `var` field"
                    )),
                }
            }
        }

        deserializer.deserialize_map(OptVisitor::default())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VarOpt {
    pub var: OptNameBuf,
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
            std::cmp::Ordering::Equal => {}
            ord => return ord,
        }
        match self.default.cmp(&other.default) {
            std::cmp::Ordering::Equal => {}
            ord => return ord,
        }
        match self.choices.iter().cmp(other.choices.iter()) {
            std::cmp::Ordering::Equal => {}
            ord => return ord,
        }
        match self.inheritance.cmp(&other.inheritance) {
            std::cmp::Ordering::Equal => {}
            ord => return ord,
        }
        self.value.cmp(&other.value)
    }
}

impl PartialOrd for VarOpt {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl VarOpt {
    pub fn new<S: AsRef<str>>(var: S) -> Result<Self> {
        Ok(Self {
            var: var.as_ref().parse()?,
            default: String::default(),
            choices: IndexSet::default(),
            inheritance: Inheritance::default(),
            value: None,
        })
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
                    Compatibility::Incompatible(format!(
                        "invalid value '{}', must be one of {:?}",
                        value, self.choices
                    ))
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

#[derive(Serialize)]
struct VarOptSchema {
    var: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    choices: Vec<String>,
    #[serde(skip_serializing_if = "Inheritance::is_default")]
    inheritance: Inheritance,
    #[serde(rename = "static", skip_serializing_if = "String::is_empty")]
    value: String,
}

impl Serialize for VarOpt {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        let mut out = VarOptSchema {
            var: self.var.to_string(),
            choices: self.choices.iter().map(String::to_owned).collect(),
            inheritance: self.inheritance,
            value: self.value.clone().unwrap_or_default(),
        };
        if !self.default.is_empty() {
            out.var = format!("{}/{}", self.var, self.default);
        }

        out.serialize(serializer)
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PkgOpt {
    pub pkg: PkgNameBuf,
    pub default: String,
    pub prerelease_policy: PreReleasePolicy,
    pub required_compat: Option<CompatRule>,
    value: Option<String>,
}

impl PkgOpt {
    pub fn new(name: PkgNameBuf) -> Result<Self> {
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
        if let Err(err) = nom::branch::alt((
            // empty string is okay in this position
            nom::combinator::eof,
            nom::combinator::recognize(nom::combinator::all_consuming(
                crate::ident::parsing::version_filter_and_build,
            )),
        ))(value.as_str())
        .map_err(|err| match err {
            nom::Err::Error(e) | nom::Err::Failure(e) => {
                crate::Error::String(nom::error::convert_error(value.as_str(), e))
            }
            nom::Err::Incomplete(_) => unreachable!(),
        }) {
            return Err(Error::wrap(
                format!(
                    "Invalid value '{}' for option '{}', not a valid package request",
                    value, self.pkg
                ),
                err,
            ));
        }
        // else accept the value
        self.value = Some(value);
        Ok(())
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
            Ok(value_range) => value_range.intersects(&base_range),
        }
    }

    pub fn to_request(
        &self,
        given_value: Option<String>,
        requester: RequestedBy,
    ) -> Result<PkgRequest> {
        let value = self.get_value(given_value.as_deref()).unwrap_or_default();
        let ident_range = if value.is_empty() {
            self.pkg.to_string()
        } else {
            format!("{}/{}", self.pkg, value)
        };
        let pkg = parse_ident_range(ident_range)?;

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
    #[serde(default, rename = "static", skip_serializing_if = "String::is_empty")]
    value: String,
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
        };
        if !self.default.is_empty() {
            out.pkg = format!("{}/{}", self.pkg, self.default);
        }

        out.serialize(serializer)
    }
}
