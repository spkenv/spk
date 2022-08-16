// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::path::Path;
use std::str::FromStr;

use enum_dispatch::enum_dispatch;
use serde::{Deserialize, Serialize};
use spk_foundation::name::{PkgName, PkgNameBuf};
use spk_foundation::option_map::OptionMap;
use spk_foundation::spec_ops::{Named, PackageOps, RecipeOps, Versioned};
use spk_foundation::version::{Compat, Compatibility, Version};
use spk_ident::{Ident, PkgRequest, RangeIdent, Request, VarRequest};

use crate::{test_spec::TestSpec, Deprecate, DeprecateMut, Error, Package, Result};
use crate::{BuildEnv, ComponentSpec, Recipe, Template, TemplateExt};

/// Create a spec recipe from a json structure.
///
/// This will panic if the given struct
/// cannot be deserialized into a recipe.
///
/// ```
/// # #[macro_use] extern crate spk_spec;
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
        let value = $crate::serde_json::json!($($spec)+);
        let spec: $crate::SpecRecipe = $crate::serde_json::from_value(value).unwrap();
        spec
    }};
}

/// Create a spec from a json structure.
///
/// This will panic if the given struct
/// cannot be deserialized into a spec.
///
/// ```
/// # #[macro_use] extern crate spk_spec;
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
        let value = $crate::serde_json::json!($($spec)+);
        let spec: $crate::Spec = $crate::serde_json::from_value(value).unwrap();
        spec
    }};
}

/// A generic, structured data object that can be turned into a recipe
/// when provided with the necessary option values
pub struct SpecTemplate {
    name: PkgNameBuf,
    file_path: std::path::PathBuf,
    inner: serde_yaml::Mapping,
}

impl Named for SpecTemplate {
    fn name(&self) -> &PkgName {
        &self.name
    }
}

impl Template for SpecTemplate {
    type Output = SpecRecipe;

    fn file_path(&self) -> &Path {
        &self.file_path
    }

    fn render(&self, _options: &OptionMap) -> Result<Self::Output> {
        serde_yaml::from_value(self.inner.clone().into()).map_err(|err| {
            Error::String(format!(
                "failed to parse rendered template for {}: {err}",
                self.file_path.display()
            ))
        })
    }
}

impl TemplateExt for SpecTemplate {
    fn from_file(path: &Path) -> Result<Self> {
        let file_path = path.canonicalize()?;
        let file = std::fs::File::open(&file_path)?;
        let reader = std::io::BufReader::new(file);

        let inner: serde_yaml::Mapping = serde_yaml::from_reader(reader).map_err(|err| {
            Error::String(format!("Invalid yaml in template file {path:?}: {err}"))
        })?;

        let pkg = inner
            .get(&serde_yaml::Value::String("pkg".to_string()))
            .ok_or_else(|| {
                crate::Error::String(format!("Missing pkg field in spec file: {file_path:?}"))
            })?;
        let pkg = pkg.as_str().ok_or_else(|| {
            crate::Error::String(format!(
                "Invalid value for 'pkg' field: expected string, got {pkg:?} in {file_path:?}"
            ))
        })?;
        let name = PkgNameBuf::from_str(
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

        Ok(Self {
            file_path,
            name,
            inner,
        })
    }
}

/// Specifies some buildable object within the spk ecosystem.
///
/// All build-able types have a recipe representation
/// that can be serialized and deserialized from a human-written
/// file or machine-managed persistent storage.
#[derive(Debug, Clone, Hash, Eq, PartialEq, Ord, PartialOrd, Serialize)]
#[serde(tag = "api")]
#[enum_dispatch(Deprecate, DeprecateMut)]
pub enum SpecRecipe {
    #[serde(rename = "v0/package")]
    V0Package(super::v0::Spec),
}

impl RecipeOps for SpecRecipe {
    type Ident = Ident;
    type PkgRequest = PkgRequest;
    type RangeIdent = RangeIdent;

    fn is_api_compatible(&self, base: &Version) -> Compatibility {
        match self {
            SpecRecipe::V0Package(r) => r.is_api_compatible(base),
        }
    }

    fn is_binary_compatible(&self, base: &Version) -> Compatibility {
        match self {
            SpecRecipe::V0Package(r) => r.is_binary_compatible(base),
        }
    }

    fn is_satisfied_by_range_ident(
        &self,
        range_ident: &Self::RangeIdent,
        required: spk_foundation::version::CompatRule,
    ) -> Compatibility {
        match self {
            SpecRecipe::V0Package(r) => r.is_satisfied_by_range_ident(range_ident, required),
        }
    }

    fn is_satisfied_by_pkg_request(&self, pkg_request: &Self::PkgRequest) -> Compatibility {
        match self {
            SpecRecipe::V0Package(r) => r.is_satisfied_by_pkg_request(pkg_request),
        }
    }

    fn to_ident(&self) -> Self::Ident {
        match self {
            SpecRecipe::V0Package(r) => r.to_ident(),
        }
    }
}

impl Recipe for SpecRecipe {
    type Output = Spec;

    fn default_variants(&self) -> &Vec<OptionMap> {
        match self {
            SpecRecipe::V0Package(r) => r.default_variants(),
        }
    }

    fn resolve_options(&self, inputs: &OptionMap) -> Result<OptionMap> {
        match self {
            SpecRecipe::V0Package(r) => r.resolve_options(inputs),
        }
    }

    fn get_build_requirements(&self, options: &OptionMap) -> Result<Vec<Request>> {
        match self {
            SpecRecipe::V0Package(r) => r.get_build_requirements(options),
        }
    }

    fn get_tests(&self, options: &OptionMap) -> Result<Vec<TestSpec>> {
        match self {
            SpecRecipe::V0Package(r) => r.get_tests(options),
        }
    }

    fn generate_source_build(&self, root: &Path) -> Result<Self::Output> {
        match self {
            SpecRecipe::V0Package(r) => r.generate_source_build(root).map(Spec::V0Package),
        }
    }

    fn generate_binary_build<E, P>(
        &self,
        options: &OptionMap,
        build_env: &E,
    ) -> Result<Self::Output>
    where
        E: BuildEnv<Package = P>,
        P: Package<Ident = Ident>,
    {
        match self {
            SpecRecipe::V0Package(r) => r
                .generate_binary_build(options, build_env)
                .map(Spec::V0Package),
        }
    }
}

impl PackageOps for SpecRecipe {
    type Ident = Ident;
    type Component = ComponentSpec;
    type VarRequest = VarRequest;

    fn components_iter(&self) -> std::slice::Iter<'_, Self::Component> {
        match self {
            SpecRecipe::V0Package(r) => r.components_iter(),
        }
    }

    fn ident(&self) -> &Self::Ident {
        match self {
            SpecRecipe::V0Package(r) => r.ident(),
        }
    }

    fn is_satisfied_by_var_request(&self, var_request: &Self::VarRequest) -> Compatibility {
        match self {
            SpecRecipe::V0Package(r) => r.is_satisfied_by_var_request(var_request),
        }
    }
}

impl Named for SpecRecipe {
    fn name(&self) -> &PkgName {
        match self {
            SpecRecipe::V0Package(r) => r.name(),
        }
    }
}

impl Versioned for SpecRecipe {
    fn version(&self) -> &Version {
        match self {
            SpecRecipe::V0Package(r) => r.version(),
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
/// All resolve-able types have a spec representation that can be serialized
/// and deserialized from a `Repository`.
#[derive(Debug, Clone, Hash, Eq, PartialEq, Ord, PartialOrd, Serialize)]
#[serde(tag = "api")]
#[enum_dispatch(Deprecate, DeprecateMut, Package)]
pub enum Spec {
    #[serde(rename = "v0/package")]
    V0Package(super::v0::Spec),
}

impl RecipeOps for Spec {
    type Ident = Ident;
    type PkgRequest = PkgRequest;
    type RangeIdent = RangeIdent;

    fn is_api_compatible(&self, base: &Version) -> Compatibility {
        match self {
            Spec::V0Package(r) => RecipeOps::is_api_compatible(r, base),
        }
    }

    fn is_binary_compatible(&self, base: &Version) -> Compatibility {
        match self {
            Spec::V0Package(r) => RecipeOps::is_binary_compatible(r, base),
        }
    }

    fn is_satisfied_by_range_ident(
        &self,
        range_ident: &Self::RangeIdent,
        required: spk_foundation::version::CompatRule,
    ) -> Compatibility {
        match self {
            Spec::V0Package(r) => RecipeOps::is_satisfied_by_range_ident(r, range_ident, required),
        }
    }

    fn is_satisfied_by_pkg_request(&self, pkg_request: &Self::PkgRequest) -> Compatibility {
        match self {
            Spec::V0Package(r) => RecipeOps::is_satisfied_by_pkg_request(r, pkg_request),
        }
    }

    fn to_ident(&self) -> Self::Ident {
        match self {
            Spec::V0Package(r) => RecipeOps::to_ident(r),
        }
    }
}

impl Recipe for Spec {
    type Output = Spec;

    fn default_variants(&self) -> &Vec<OptionMap> {
        match self {
            Spec::V0Package(r) => r.default_variants(),
        }
    }

    fn resolve_options(&self, inputs: &OptionMap) -> Result<OptionMap> {
        match self {
            Spec::V0Package(r) => r.resolve_options(inputs),
        }
    }

    fn get_build_requirements(&self, options: &OptionMap) -> Result<Vec<Request>> {
        match self {
            Spec::V0Package(r) => r.get_build_requirements(options),
        }
    }

    fn get_tests(&self, options: &OptionMap) -> Result<Vec<TestSpec>> {
        match self {
            Spec::V0Package(r) => r.get_tests(options),
        }
    }

    fn generate_source_build(&self, root: &Path) -> Result<Self::Output> {
        match self {
            Spec::V0Package(r) => r.generate_source_build(root).map(Spec::V0Package),
        }
    }

    fn generate_binary_build<E, P>(
        &self,
        options: &OptionMap,
        build_env: &E,
    ) -> Result<Self::Output>
    where
        E: BuildEnv<Package = P>,
        P: Package<Ident = Ident>,
    {
        match self {
            Spec::V0Package(r) => r
                .generate_binary_build(options, build_env)
                .map(Spec::V0Package),
        }
    }
}

impl PackageOps for Spec {
    type Ident = Ident;
    type Component = ComponentSpec;
    type VarRequest = VarRequest;

    fn components_iter(&self) -> std::slice::Iter<'_, Self::Component> {
        match self {
            Spec::V0Package(r) => PackageOps::components_iter(r),
        }
    }

    fn ident(&self) -> &Self::Ident {
        match self {
            Spec::V0Package(r) => PackageOps::ident(r),
        }
    }

    fn is_satisfied_by_var_request(&self, var_request: &Self::VarRequest) -> Compatibility {
        match self {
            Spec::V0Package(r) => PackageOps::is_satisfied_by_var_request(r, var_request),
        }
    }
}

impl Named for Spec {
    fn name(&self) -> &PkgName {
        match self {
            Spec::V0Package(r) => r.name(),
        }
    }
}

impl Versioned for Spec {
    fn version(&self) -> &Version {
        match self {
            Spec::V0Package(r) => r.version(),
        }
    }
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
