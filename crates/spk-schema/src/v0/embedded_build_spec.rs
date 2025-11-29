// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::hash::Hash;

use serde::{Deserialize, Serialize};
use serde_yaml::Value;
use spk_schema_foundation::IsDefault;
use spk_schema_foundation::name::PkgName;
use spk_schema_foundation::option_map::OptionMap;
use spk_schema_foundation::spec_ops::Versioned;

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

impl EmbeddedBuildSpec {
    /// Render all requests with a package pin using the given resolved packages.
    pub fn render_all_pins<K, R>(
        self,
        options: &OptionMap,
        resolved_by_name: &std::collections::HashMap<K, R>,
    ) -> Result<Self>
    where
        K: Eq + std::hash::Hash,
        K: std::borrow::Borrow<PkgName>,
        R: Versioned,
    {
        Ok(Self {
            options: self
                .options
                .into_iter()
                .map(|opt| {
                    opt.render_all_pins(
                        options,
                        resolved_by_name,
                        // An embedded package that says it depends on a package
                        // "foo" that the parent package doesn't have in its build
                        // requirements means that "foo" will not necessarily be in
                        // the build env. That shouldn't prevent the parent package
                        // from building, but also the embedded stub will not get
                        // its build requirement for "foo" pinned. It is impossible
                        // to "build" an embedded package anyway; build pkg
                        // requirements in an embedded package are basically
                        // meaningless.
                        false,
                    )
                })
                .collect::<Result<Vec<_>>>()?,
        })
    }
}

impl IsDefault for EmbeddedBuildSpec {
    fn is_default(&self) -> bool {
        self == &Self::default()
    }
}
