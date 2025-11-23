// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::hash::Hash;

use serde::{Deserialize, Serialize};
use serde_yaml::Value;
use spk_schema_foundation::IsDefault;

use crate::{Error, Opt, Result};

#[cfg(test)]
#[path = "./embedded_build_spec_test.rs"]
mod embedded_build_spec_test;

#[derive(Deserialize)]
struct RawEmbeddedBuildSpec {
    options: Vec<Opt>,
    script: Option<Value>,
    variants: Option<Value>,
    auto_host_vars: Option<Value>,
}

impl TryFrom<RawEmbeddedBuildSpec> for EmbeddedBuildSpec {
    type Error = Error;

    fn try_from(raw: RawEmbeddedBuildSpec) -> Result<Self> {
        if raw.script.is_some() {
            return Err(Error::String(
                "embedded build spec cannot contain a build script".to_owned(),
            ));
        }
        if raw.variants.is_some() {
            return Err(Error::String(
                "embedded build spec cannot contain variants".to_owned(),
            ));
        }
        if raw.auto_host_vars.is_some() {
            return Err(Error::String(
                "embedded build spec cannot contain auto_host_vars".to_owned(),
            ));
        }
        Ok(EmbeddedBuildSpec {
            options: raw.options,
        })
    }
}

/// A set of structured inputs used to build a package.
#[derive(Clone, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(try_from = "RawEmbeddedBuildSpec")]
pub struct EmbeddedBuildSpec {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<Opt>,
}

impl IsDefault for EmbeddedBuildSpec {
    fn is_default(&self) -> bool {
        self == &Self::default()
    }
}
