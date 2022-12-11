// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use serde::{Deserialize, Serialize};
use spk_schema_foundation::name::{OptName, OptNameBuf};
use spk_schema_ident::{NameAndValue, RangeIdent};

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
            Self::Var(v) => &v.var.0,
        }
    }

    pub fn value(&self) -> String {
        match self {
            Self::Pkg(p) => p.pkg.version.to_string(),
            Self::Var(v) => v.var.1.clone().unwrap_or_default(),
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
    #[serde(default, skip_serializing_if = "is_false")]
    pub at_runtime: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub at_downstream_build: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub at_downstream_runtime: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
#[cfg_attr(test, serde(deny_unknown_fields))]
#[serde(rename_all = "camelCase")]
pub struct PkgOption {
    pub pkg: RangeIdent,
    #[serde(default, skip_serializing_if = "is_false")]
    pub at_runtime: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub at_downstream_build: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub at_downstream_runtime: bool,
}

fn is_false(v: &bool) -> bool {
    !*v
}
