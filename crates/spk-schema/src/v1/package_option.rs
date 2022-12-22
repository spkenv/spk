// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::str::FromStr;

use serde::{Deserialize, Serialize};
use spk_schema_foundation::name::{OptName, OptNameBuf};
use spk_schema_foundation::spec_ops::HasVersion;
use spk_schema_foundation::version::Compatibility;
use spk_schema_foundation::version_range::{CompatRange, Ranged, VersionRange};
use spk_schema_ident::{
    NameAndValue,
    PkgRequest,
    RangeIdent,
    Request,
    RequestedBy,
    Satisfy,
    VarRequest,
};

use super::ConditionOutcome;
use crate::{BuildEnv, BuildEnvMember, Error, Result};

#[cfg(test)]
#[path = "./package_option_test.rs"]
mod package_option_test;

#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PackageOption {
    Var(Box<VarOption>),
    Pkg(Box<PkgOption>),
}

impl PackageOption {
    pub fn name(&self) -> &OptName {
        match self {
            Self::Pkg(p) => p.pkg.name.as_opt_name(),
            Self::Var(v) => v.var.name(),
        }
    }

    pub fn value(&self, given: Option<&String>) -> String {
        match self {
            Self::Pkg(p) => p.pkg.version.to_string(),
            Self::Var(v) => v
                .var
                .value(given)
                .map(ToString::to_string)
                .unwrap_or_default(),
        }
    }

    pub fn propagation(&self) -> &OptionPropagation {
        match self {
            Self::Pkg(p) => &p.propagation,
            Self::Var(v) => &v.propagation,
        }
    }

    pub fn to_request(
        &self,
        given: Option<&String>,
        requested_by: impl FnOnce() -> RequestedBy,
    ) -> Option<Request> {
        match self {
            Self::Pkg(p) => Some(Request::Pkg(p.to_request(requested_by()))),
            Self::Var(v) => v.to_request(given).map(Request::Var),
        }
    }

    pub fn as_var(&self) -> Option<&VarOption> {
        match self {
            Self::Var(v) => Some(v),
            _ => None,
        }
    }

    pub fn as_pkg(&self) -> Option<&PkgOption> {
        match self {
            Self::Pkg(v) => Some(v),
            _ => None,
        }
    }

    pub fn validate(&self, value: Option<&str>) -> Compatibility {
        match self {
            Self::Pkg(p) => p.validate(value),
            Self::Var(v) => v.validate(value),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[cfg_attr(test, serde(deny_unknown_fields))]
#[serde(rename_all = "camelCase")]
pub struct VarOption {
    pub var: NameAndValue<OptNameBuf>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub choices: Vec<String>,
    #[serde(flatten)]
    pub propagation: OptionPropagation,
}

impl VarOption {
    pub fn new<E>(opt: &super::VarOption, build_env: E) -> Self
    where
        E: BuildEnv,
        E::Package: Satisfy<PkgRequest>,
    {
        let propagation = super::package_option::OptionPropagation {
            at_runtime: opt.at_runtime.check_is_active(&build_env),
            at_downstream: opt.at_downstream.check_is_active(&build_env),
        };
        let build_options = build_env.options();
        let name = || opt.var.name().clone();
        let var = build_options
            .get_for_package(build_env.target().name(), opt.var.name())
            .or_else(|| opt.var.value(build_options.get(opt.var.name())))
            .map(|v| NameAndValue::WithAssignedValue(name(), v.clone()))
            .unwrap_or_else(|| NameAndValue::NameOnly(name()));
        super::package_option::VarOption {
            var,
            choices: opt.choices.clone(),
            propagation,
        }
    }

    pub fn to_request(&self, given: Option<&String>) -> Option<VarRequest> {
        self.var.value(given).map(|value| VarRequest {
            var: self.var.name().clone(),
            pin: false,
            value: value.to_owned(),
        })
    }

    pub fn validate(&self, value: Option<&str>) -> Compatibility {
        let Some(value) = value else {
            let default = self.var.value(None);
            return self.validate(default.map(String::as_str));
        };
        match &self.var {
            NameAndValue::NameOnly(_) => Compatibility::Compatible,
            NameAndValue::WithDefaultValue(_, _v) => Compatibility::Compatible,
            NameAndValue::WithAssignedValue(_, v) if value == v || value.is_empty() => {
                Compatibility::Compatible
            }
            NameAndValue::WithAssignedValue(_, v) => Compatibility::incompatible(format!(
                "incompatible option, wanted '{v}', got '{value:?}'",
            )),
        }
    }
}

impl Satisfy<VarRequest> for VarOption {
    fn check_satisfies_request(&self, var_request: &VarRequest) -> Compatibility {
        if self.var.name() != &var_request.var {
            return Compatibility::incompatible(format!(
                "request is for an entirely different var: want: '{}', got: '{}'",
                self.var.name(),
                var_request.var
            ));
        }
        let needed_value = self.var.value_or_default();
        if needed_value != var_request.value {
            return Compatibility::incompatible(format!(
                "request is for an entirely different var: want: '{}', got: '{}'",
                self.var, var_request.value
            ));
        }
        Compatibility::Compatible
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[cfg_attr(test, serde(deny_unknown_fields))]
#[serde(rename_all = "camelCase")]
pub struct PkgOption {
    pub pkg: RangeIdent,
    #[serde(flatten)]
    pub propagation: OptionPropagation,
}

impl PkgOption {
    /// Create a PkgOption from a recipe option and build environment.
    ///
    /// # Errors
    /// - if the package that `recipe_opt` is for does not
    ///   appear in the given build environment
    pub fn new<E>(recipe_opt: &super::PkgOption, build_env: E) -> Result<Self>
    where
        E: BuildEnv,
        E::Package: Satisfy<PkgRequest>,
    {
        let propagation = super::package_option::OptionPropagation {
            at_runtime: recipe_opt
                .at_runtime
                .check_is_active(&recipe_opt.pkg, &build_env),
            at_downstream: recipe_opt
                .at_downstream
                .check_is_active(&recipe_opt.pkg, &build_env),
        };
        let resolved = build_env.get_member(recipe_opt.pkg.name()).ok_or_else(|| Error::String(format!("Cannot compute package option for '{}': it was not resolved in the build environment", recipe_opt.pkg.name())))?;
        let resolved_version = resolved.package().version().clone();
        Ok(Self {
            pkg: RangeIdent::new(
                recipe_opt.pkg.name(),
                // TODO: support other versions/components at runtime?
                CompatRange::new(resolved_version, None).into(),
                recipe_opt.pkg.components.iter().cloned(),
            ),
            propagation,
        })
    }

    pub fn to_request(&self, requested_by: RequestedBy) -> PkgRequest {
        PkgRequest::new(self.pkg.clone(), requested_by)
    }

    pub fn validate(&self, value: Option<&str>) -> Compatibility {
        let value = value.unwrap_or_default();

        match VersionRange::from_str(value) {
            Err(err) => Compatibility::incompatible(format!(
                "Invalid value '{}' for option '{}', not a valid package request: {}",
                value, self.pkg, err
            )),
            Ok(value_range) => value_range.intersects(&self.pkg.version),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[cfg_attr(test, serde(deny_unknown_fields))]
#[serde(rename_all = "camelCase")]
pub struct OptionPropagation {
    #[serde(default)]
    pub at_runtime: ConditionOutcome,
    #[serde(default)]
    pub at_downstream: ConditionOutcome,
}
