// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::collections::HashSet;
use std::convert::TryFrom;

use pyo3::prelude::*;
use serde::{Deserialize, Serialize};

use super::{
    parse_ident_range, CompatRule, Compatibility, InclusionPolicy, OptionMap, PkgRequest,
    PreReleasePolicy, Request, VarRequest,
};
use crate::{Error, Result};

#[cfg(test)]
#[path = "./build_spec_test.rs"]
mod build_spec_test;

/// An option that can be provided to provided to the package build process
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, FromPyObject)]
#[serde(untagged)]
pub enum Opt {
    Pkg(PkgOpt),
    Var(VarOpt),
}

impl Opt {
    pub fn name<'a>(&'a self) -> &'a str {
        match self {
            Self::Pkg(opt) => &opt.pkg,
            Self::Var(opt) => &opt.var,
        }
    }

    pub fn namespaced_name<S: AsRef<str>>(&self, pkg: S) -> String {
        match self {
            Self::Pkg(opt) => opt.namespaced_name(pkg),
            Self::Var(opt) => opt.namespaced_name(pkg),
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
    pub fn get_value(&self, given: &Option<String>) -> String {
        let value = match self {
            Self::Pkg(opt) => opt.get_value(given),
            Self::Var(opt) => opt.get_value(given),
        };
        match (value, given) {
            (Some(v), _) => v,
            (_, Some(v)) => v.clone(),
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
                    .skip(request.pkg.name().len())
                    .skip(1)
                    .collect();
                Ok(Opt::Pkg(PkgOpt {
                    pkg: request.pkg.name().to_owned(),
                    default: default,
                    prerelease_policy: request.prerelease_policy,
                    value: None,
                }))
            }
            Request::Var(_) => Err(Error::String(format!(
                "Cannot convert {:?} to option",
                request
            ))),
        }
    }
}

/// A set of structured inputs used to build a package.
#[pyclass]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BuildSpec {
    pub script: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<Opt>,
    #[serde(default, skip_serializing_if = "BuildSpec::is_default_variants")]
    pub variants: Vec<OptionMap>,
}

impl Default for BuildSpec {
    fn default() -> Self {
        Self {
            script: vec!["sh ./build.sh".into()],
            options: Vec::new(),
            variants: vec![OptionMap::default()],
        }
    }
}

impl BuildSpec {
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }

    fn is_default_variants(variants: &Vec<OptionMap>) -> bool {
        if variants.len() != 1 {
            return false;
        }
        variants.get(0) == Some(&OptionMap::default())
    }

    pub fn resolve_all_options(&self, package_name: Option<&str>, given: &OptionMap) -> OptionMap {
        let mut resolved = OptionMap::default();
        for opt in self.options.iter() {
            let name = opt.name();
            let mut given_value: Option<&String> = None;

            if let Some(name) = &package_name {
                given_value = given.get(&opt.namespaced_name(name))
            }
            if let None = &given_value {
                given_value = given.get(name)
            }

            let value = opt.get_value(&given_value.map(String::to_owned));
            resolved.insert(name.to_string(), value);
        }

        resolved
    }

    /// Validate the given options against the options in this spec.
    pub fn validate_options<S: AsRef<str>>(
        &self,
        package_name: S,
        mut given_options: OptionMap,
    ) -> Compatibility {
        let mut must_exist = given_options.package_options_without_global(&package_name);
        given_options = given_options.package_options(&package_name);
        for option in self.options.iter() {
            let compat = option.validate(given_options.get(option.name()).map(String::as_str));
            if !compat.is_ok() {
                return Compatibility::Incompatible(format!(
                    "invalid value for {}: {}",
                    option.name(),
                    compat
                ));
            }

            must_exist.remove(option.name());
        }

        let missing = must_exist.keys();
        if missing.len() != 0 {
            let missing = must_exist.iter().collect::<Vec<_>>();
            return Compatibility::Incompatible(format!(
                "Package does not define requested build options: {:?}",
                missing
            ));
        }

        Compatibility::Compatible
    }

    /// Add or update an option in this build spec.
    ///
    /// An option is replaced if it shares a name with the given option,
    /// otherwise the option is appended to the buid options
    pub fn upsert_opt(&mut self, opt: Opt) {
        for other in self.options.iter_mut() {
            if other.name() == opt.name() {
                let _ = std::mem::replace(other, opt);
                return;
            }
        }
        self.options.push(opt);
    }
}

impl<'de> Deserialize<'de> for BuildSpec {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Unchecked {
            #[serde(default, skip_serializing_if = "Option::is_none")]
            script: Option<Vec<String>>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            options: Option<Vec<Opt>>,
            #[serde(default, skip_serializing_if = "BuildSpec::is_default_variants")]
            variants: Vec<OptionMap>,
        }

        let raw = Unchecked::deserialize(deserializer)?;
        let mut bs = BuildSpec::default();
        if let Some(script) = raw.script {
            bs.script = script
        }
        if let Some(options) = raw.options {
            bs.options = options
        }
        if !raw.variants.is_empty() {
            bs.variants = raw.variants
        }
        let mut unique_options = HashSet::new();
        for opt in bs.options.iter() {
            let name = opt.name();
            if unique_options.contains(&name) {
                return Err(serde::de::Error::custom(format!(
                    "Build option specified more than once: {}",
                    opt.name()
                )));
            }
            unique_options.insert(name);
        }

        let mut variant_builds = Vec::new();
        let mut unique_variants = HashSet::new();
        for variant in bs.variants.iter() {
            let mut build_opts = variant.clone();
            build_opts.append(&mut bs.resolve_all_options(None, variant));
            let digest = build_opts.digest();
            variant_builds.push((digest.clone(), variant.clone()));
            unique_variants.insert(digest);
        }
        if unique_variants.len() < variant_builds.len() {
            let details = variant_builds
                .iter()
                .map(|(h, o)| format!("- {} ({:?})", o, h))
                .collect::<Vec<_>>()
                .join("\n");
            return Err(serde::de::Error::custom(format!(
                "Multiple variants would produce the same build:\n{}",
                details
            )));
        }

        Ok(bs)
    }
}

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

#[pyclass]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VarOpt {
    #[pyo3(get, set)]
    pub var: String,
    #[pyo3(get, set)]
    pub default: String,
    #[pyo3(get, set)]
    pub choices: HashSet<String>,
    #[pyo3(get, set)]
    pub inheritance: Inheritance,
    #[pyo3(get)]
    value: Option<String>,
}

impl VarOpt {
    pub fn new<S: AsRef<str>>(var: S) -> Self {
        Self {
            var: var.as_ref().to_string(),
            default: String::default(),
            choices: HashSet::default(),
            inheritance: Inheritance::default(),
            value: None,
        }
    }
    pub fn namespaced_name<S: AsRef<str>>(&self, pkg: S) -> String {
        if self.var.contains(".") {
            self.var.clone()
        } else {
            format!("{}.{}", pkg.as_ref(), self.var)
        }
    }

    pub fn get_value(&self, given: &Option<String>) -> Option<String> {
        if let Some(v) = &self.value {
            Some(v.clone())
        } else if let Some(v) = given {
            Some(v.clone())
        } else {
            Some(self.default.clone())
        }
    }

    pub fn set_value(&mut self, value: String) -> Result<()> {
        if self.choices.len() > 0 && !value.is_empty() {
            if !self.choices.contains(&value) {
                return Err(Error::String(format!(
                    "Invalid value '{}' for option '{}', must be one of {:?}",
                    value, self.var, self.choices
                )));
            }
        }
        self.value = Some(value);
        Ok(())
    }

    pub fn validate(&self, value: Option<&str>) -> Compatibility {
        if value.is_none() {
            return self.validate(self.value.as_ref().map(String::as_str));
        }
        let assigned = self.get_value(&None);
        match (value, assigned) {
            (None, Some(_)) => Compatibility::Compatible,
            (Some(value), Some(assigned)) => {
                if value == assigned {
                    return Compatibility::Compatible;
                } else {
                    Compatibility::Incompatible(format!(
                        "incompatible option, wanted '{}', got '{}'",
                        value, assigned
                    ))
                }
            }
            (Some(value), _) => {
                if self.choices.len() > 0 && !self.choices.contains(value) {
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

    pub fn to_request(self, given_value: &Option<String>) -> VarRequest {
        let value = self.get_value(given_value).unwrap_or_default();
        return VarRequest {
            var: self.var,
            value: value,
            pin: false,
        };
    }
}

#[pymethods]
impl VarOpt {
    #[new]
    fn init(var: &str) -> Self {
        Self::new(var)
    }
}

#[derive(Serialize, Deserialize)]
struct VarOptSchema {
    var: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    choices: Vec<String>,
    #[serde(default, skip_serializing_if = "Inheritance::is_default")]
    inheritance: Inheritance,
    #[serde(default, rename = "static", skip_serializing_if = "String::is_empty")]
    value: String,
    // the default field can be loaded for legacy compatibility but is deprecated
    #[serde(default, skip_serializing)]
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
            out.default = default.clone();
        } else {
            let mut split = data.var.split("/");
            out.var = split.next().unwrap().to_string();
            out.default = split.collect::<Vec<_>>().join("");
        }

        if !data.value.is_empty() {
            out.value = Some(data.value.to_owned());
        }
        Ok(out)
    }
}

#[pyclass]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PkgOpt {
    #[pyo3(get, set)]
    pub pkg: String,
    #[pyo3(get, set)]
    pub default: String,
    #[pyo3(get, set)]
    pub prerelease_policy: PreReleasePolicy,
    #[pyo3(get)]
    value: Option<String>,
}

impl PkgOpt {
    pub fn get_value(&self, given: &Option<String>) -> Option<String> {
        if let Some(v) = &self.value {
            Some(v.clone())
        } else if let Some(v) = given {
            Some(v.clone())
        } else {
            Some(self.default.clone())
        }
    }

    pub fn set_value(&mut self, value: String) -> Result<()> {
        let ident = format!("{}/{}", self.pkg, value);
        if let Err(err) = parse_ident_range(ident) {
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

    pub fn namespaced_name<S: AsRef<str>>(&self, pkg: S) -> String {
        format!("{}.{}", pkg.as_ref(), self.pkg)
    }

    pub fn validate(&self, value: Option<&str>) -> Compatibility {
        let value = value.unwrap_or_default();

        // skip any default that might exist since
        // that does not represent a definitive range
        let base = self.value.as_ref().map(String::as_str).unwrap_or_default();
        let base_range = match parse_ident_range(format!("{}/{}", self.pkg, base)) {
            Err(err) => {
                return Compatibility::Incompatible(format!(
                    "Invalid value '{}' for option '{}', not a valid package request: {}",
                    base, self.pkg, err
                ))
            }
            Ok(r) => r,
        };
        match parse_ident_range(format!("{}/{}", self.pkg, value)) {
            Err(err) => Compatibility::Incompatible(format!(
                "Invalid value '{}' for option '{}', not a valid package request: {}",
                value, self.pkg, err
            )),
            Ok(value_range) => value_range.contains(&base_range),
        }
    }

    pub fn to_request(&self, given_value: Option<String>) -> Result<Request> {
        let mut value = self.default.clone();
        if let Some(given_value) = given_value {
            value = given_value;
        }
        Ok(Request::Pkg(PkgRequest {
            pkg: parse_ident_range(format!("{}/{}", self.pkg, value))?,
            pin: None,
            prerelease_policy: self.prerelease_policy,
            inclusion_policy: InclusionPolicy::default(),
            required_compat: CompatRule::API,
        }))
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
    // the default field can be loaded for legacy compatibility but is deprecated
    #[serde(default, skip_serializing)]
    default: Option<String>,
}

impl Serialize for PkgOpt {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        let mut out = PkgOptSchema {
            pkg: self.pkg.clone(),
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
        let mut out = PkgOpt {
            pkg: data.pkg.clone(),
            default: "".to_string(),
            prerelease_policy: data.prerelease_policy,
            value: None,
        };
        if let Some(default) = data.default {
            // the default field is deprecated, but we support it for existing packages
            out.default = default.to_owned();
        } else {
            let mut split = data.pkg.split("/");
            out.pkg = split.next().unwrap().to_string();
            out.default = split.collect::<Vec<_>>().join("");
        }

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
