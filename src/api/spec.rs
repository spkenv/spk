// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::str::FromStr;

use enum_dispatch::enum_dispatch;
use serde::{Deserialize, Serialize};

use super::{Deprecate, DeprecateMut, Named, Package, Template, Versioned};
use crate::{Error, Result};

/// Create a spec recipe from a json structure.
///
/// This will panic if the given struct
/// cannot be deserialized into a recipe.
///
/// ```
/// # #[macro_use] extern crate spk;
/// # fn main() {
/// recipe!({
///   "api": "v0/package",
///   "pkg": "my-pkg/1.0.0",
///   "build": {
///     "options": [
///       {"pkg": "dependency"}
///     ]
///   }
/// });
/// # }
/// ```
#[macro_export]
macro_rules! recipe {
    ($($spec:tt)+) => {{
        let value = serde_json::json!($($spec)+);
        let spec: $crate::api::SpecRecipe = serde_json::from_value(value).unwrap();
        spec
    }};
}

/// Create a spec from a json structure.
///
/// This will panic if the given struct
/// cannot be deserialized into a spec.
///
/// ```
/// # #[macro_use] extern crate spk;
/// # fn main() {
/// spec!({
///   "api": "v0/package",
///   "pkg": "my-pkg/1.0.0",
///   "build": {
///     "options": [
///       {"pkg": "dependency"}
///     ]
///   }
/// });
/// # }
/// ```
#[macro_export]
macro_rules! spec {
    ($($spec:tt)+) => {{
        let value = serde_json::json!($($spec)+);
        let spec: $crate::api::Spec = serde_json::from_value(value).unwrap();
        spec
    }};
}

/// A generic, structured data object that can be turned into a recipe
/// when provided with the necessary option values
pub struct SpecTemplate {
    name: super::PkgNameBuf,
    inner: serde_yaml::Mapping,
}

impl Named for SpecTemplate {
    fn name(&self) -> &super::PkgName {
        &self.name
    }
}

impl Template for SpecTemplate {
    type Output = SpecRecipe;

    fn from_file(path: &std::path::Path) -> Result<Self> {
        let filepath = path.canonicalize()?;
        let file = std::fs::File::open(&filepath)?;
        let reader = std::io::BufReader::new(file);

        let inner: serde_yaml::Mapping = serde_yaml::from_reader(reader).map_err(|err| {
            Error::String(format!("Invalid yaml in template file {path:?}: {err}"))
        })?;

        let pkg = inner
            .get(&serde_yaml::Value::String("pkg".to_string()))
            .ok_or_else(|| {
                crate::Error::String(format!("Missing pkg field in spec file: {filepath:?}"))
            })?;
        let pkg = pkg.as_str().ok_or_else(|| {
            crate::Error::String(format!(
                "Invalid value for 'pkg' field: expected string, got {pkg:?} in {filepath:?}"
            ))
        })?;
        let name = super::PkgNameBuf::from_str(
            // it should never be possible for split to return 0 results
            // but this trick avoids the use of unwrap
            pkg.split('/').next().unwrap_or(pkg),
        )?;

        if inner
            .get(&serde_yaml::Value::String("api".to_string()))
            .is_none()
        {
            tracing::warn!(
                "Spec file is missing the 'api' field, this may be an error in the future"
            );
            tracing::warn!(" > for specs in the original spk format, add 'api: v0/package'");
        }

        Ok(Self { name, inner })
    }

    /// Save this template to a file on disk
    ///
    /// If this file already exists, it will be overwritten
    fn to_file(&self, path: &std::path::Path) -> Result<()> {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .open(&path)?;
        serde_yaml::to_writer(file, &self.inner)
            .map_err(|err| Error::String(format!("Failed to save spec to file {path:?}: {err}")))
    }

    fn render(&self, _options: &super::OptionMap) -> Result<Self::Output> {
        serde_yaml::from_value(self.inner.clone().into())
            .map_err(|err| Error::String(format!("failed to parse rendered template: {err}")))
    }
}

/// Specifies some buildable object within the spk ecosystem.
///
/// All build-able types have a recipe representation
/// that can be serialized and deserialized from a human-written
/// file or machine-managed persistent storage.
#[derive(Debug, Clone, Hash, Eq, PartialEq, Ord, PartialOrd, Serialize)]
#[serde(tag = "api")]
#[enum_dispatch(Named, Versioned, Deprecate, DeprecateMut)]
pub enum SpecRecipe {
    #[serde(rename = "v0/package")]
    V0Package(super::v0::Spec),
}

impl super::Recipe for SpecRecipe {
    type Output = Spec;

    fn default_variants(&self) -> &Vec<super::OptionMap> {
        match self {
            SpecRecipe::V0Package(r) => r.default_variants(),
        }
    }

    fn resolve_options(&self, inputs: &super::OptionMap) -> Result<super::OptionMap> {
        match self {
            SpecRecipe::V0Package(r) => r.resolve_options(inputs),
        }
    }

    fn get_build_requirements(&self, options: &super::OptionMap) -> Result<Vec<super::Request>> {
        match self {
            SpecRecipe::V0Package(r) => r.get_build_requirements(options),
        }
    }

    fn get_tests(&self, options: &super::OptionMap) -> Result<Vec<super::TestSpec>> {
        match self {
            SpecRecipe::V0Package(r) => r.get_tests(options),
        }
    }

    fn generate_source_build(&self) -> Result<Self::Output> {
        match self {
            SpecRecipe::V0Package(r) => r.generate_source_build().map(Spec::V0Package),
        }
    }

    fn generate_binary_build(
        &self,
        options: &super::OptionMap,
        build_env: &crate::solve::Solution,
    ) -> Result<Self::Output> {
        match self {
            SpecRecipe::V0Package(r) => r
                .generate_binary_build(options, build_env)
                .map(Spec::V0Package),
        }
    }
}

impl<'de> Deserialize<'de> for SpecRecipe {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let mut value = serde_yaml::Mapping::deserialize(deserializer)?;
        let api_field = serde_yaml::Value::String(String::from("api"));
        // unfortunately, serde does not have a derive mechanism which
        // would allow us to specify a default enum variant for when
        // the 'api' field does not exist in a spec. This small setup will not
        // create as nice of error messages in some cases, but is the
        // best implementation that I could think of without adding a
        // non-trivial maintenance burden to the setup.
        let variant = value
            .remove(&api_field)
            .unwrap_or_else(|| serde_yaml::Value::String(String::from("v0/package")));
        match variant.as_str() {
            Some("v0/package") => Ok(Self::V0Package(
                serde_yaml::from_value(value.into()).map_err(serde::de::Error::custom)?,
            )),
            Some(variant) => Err(serde::de::Error::custom(format!(
                "Unknown api variant: '{variant}'"
            ))),
            None => Err(serde::de::Error::custom(
                "Invalid value for field 'api', expected string type",
            )),
        }
    }
}

/// Specifies some data object within the spk ecosystem.
///
/// All resolve-able types have a spec representation
/// that can be serialized and deserialized from a
/// [`crate::storage::Repository`].
#[derive(Debug, Clone, Hash, Eq, PartialEq, Ord, PartialOrd, Serialize)]
#[serde(tag = "api")]
#[enum_dispatch(Named, Versioned, Deprecate, DeprecateMut, Package)]
pub enum Spec {
    #[serde(rename = "v0/package")]
    V0Package(super::v0::Spec),
}

impl<'de> Deserialize<'de> for Spec {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let mut value = serde_yaml::Mapping::deserialize(deserializer)?;
        let api_field = serde_yaml::Value::String(String::from("api"));
        // unfortunately, serde does not have a derive mechanism which
        // would allow us to specify a default enum variant for when
        // the 'api' field does not exist in a spec. This small setup will not
        // create as nice of error messages in some cases, but is the
        // best implementation that I could think of without adding a
        // non-trivial maintenance burden to the setup.
        let variant = value
            .remove(&api_field)
            .unwrap_or_else(|| serde_yaml::Value::String(String::from("v0/package")));
        match variant.as_str() {
            Some("v0/package") => Ok(Self::V0Package(
                serde_yaml::from_value(value.into()).map_err(serde::de::Error::custom)?,
            )),
            Some(variant) => Err(serde::de::Error::custom(format!(
                "Unknown api variant: '{variant}'"
            ))),
            None => Err(serde::de::Error::custom(
                "Invalid value for field 'api', expected string type",
            )),
        }
    }
}

impl AsRef<Spec> for Spec {
    fn as_ref(&self) -> &Spec {
        self
    }
}
