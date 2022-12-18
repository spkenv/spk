// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use serde::{Deserialize, Serialize};
use spk_schema_foundation::name::{OptName, OptNameBuf};
use spk_schema_foundation::version::Compatibility;
use spk_schema_ident::{
    NameAndValue,
    PkgRequest,
    RangeIdent,
    Request,
    RequestedBy,
    Satisfy,
    VarRequest,
};

#[cfg(test)]
#[path = "./package_option_test.rs"]
mod package_option_test;

#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(tag = "kind")]
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
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
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
    pub fn to_request(&self, given: Option<&String>) -> Option<VarRequest> {
        self.var.value(given).map(|value| VarRequest {
            var: self.var.name().clone(),
            pin: false,
            value: value.to_owned(),
        })
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

#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
#[cfg_attr(test, serde(deny_unknown_fields))]
#[serde(rename_all = "camelCase")]
pub struct PkgOption {
    pub pkg: RangeIdent,
    #[serde(flatten)]
    pub propagation: OptionPropagation,
}

impl PkgOption {
    pub fn to_request(&self, requested_by: RequestedBy) -> PkgRequest {
        PkgRequest::new(self.pkg.clone(), requested_by)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
#[cfg_attr(test, serde(deny_unknown_fields))]
#[serde(rename_all = "camelCase")]
pub struct OptionPropagation {
    #[serde(default, skip_serializing_if = "is_false")]
    pub at_runtime: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub at_downstream: bool,
}

fn is_false(v: &bool) -> bool {
    !*v
}
