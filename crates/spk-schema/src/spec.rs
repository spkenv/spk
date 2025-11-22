// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::borrow::Cow;
use std::collections::HashSet;
use std::io::Read;
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;

use enum_dispatch::enum_dispatch;
use format_serde_error::SerdeError;
use serde::{Deserialize, Serialize};
use spk_schema_foundation::SerdeYamlError;
use spk_schema_foundation::ident::{BuildIdent, VersionIdent};
use spk_schema_foundation::ident_build::{Build, BuildId};
use spk_schema_foundation::ident_component::Component;
use spk_schema_foundation::option_map::OptFilter;

use crate::foundation::name::{PkgName, PkgNameBuf};
use crate::foundation::option_map::OptionMap;
use crate::foundation::spec_ops::prelude::*;
use crate::foundation::version::{Compat, Compatibility, Version};
use crate::ident::{PkgRequest, Request, Satisfy, VarRequest};
use crate::metadata::Meta;
use crate::{
    BuildEnv,
    Deprecate,
    DeprecateMut,
    Error,
    FromYaml,
    InputVariant,
    Opt,
    Package,
    PackageMut,
    Recipe,
    RequirementsList,
    Result,
    RuntimeEnvironment,
    Template,
    TemplateExt,
    Test,
    TestStage,
    Variant,
    v0,
};

#[cfg(test)]
#[path = "./spec_test.rs"]
mod spec_test;

/// Create a spec recipe from a json structure.
///
/// This will return a Result with SerdeError if the given struct
/// cannot be deserialized into a recipe.
///
/// ```
/// # #[macro_use] extern crate spk_schema;
/// # #[macro_use] extern crate format_serde_error;
/// # fn main()  -> Result<(), format_serde_error::SerdeError> {
/// let recipe = try_recipe!({
///   "api": "v0/package",
///   "pkg": "my-pkg/1.0.0",
///   "build": {
///     "options": [
///       {"pkg": "dependency"}
///     ]
///   }
/// })?;
/// # Ok(())
/// # }
/// ```
#[macro_export]
macro_rules! try_recipe {
    ($($spec:tt)+) => {{
        use $crate::FromYaml;
        let value = $crate::serde_json::json!($($spec)+);
        $crate::SpecRecipe::from_yaml(value.to_string())
    }};
}

/// Create a spec recipe from a json structure.
///
/// This will panic if the given struct
/// cannot be deserialized into a recipe.
///
/// ```
/// # #[macro_use] extern crate spk_schema;
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
        $crate::try_recipe!($($spec)+).expect("invalid recipe data")
    }};
}

/// Create a spec from a json structure.
///
/// This will panic if the given struct
/// cannot be deserialized into a spec.
///
/// ```
/// # #[macro_use] extern crate spk_schema;
/// # fn main() {
/// spec!({
///   "api": "v0/package",
///   "pkg": "my-pkg/1.0.0/src",
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
        use $crate::FromYaml;
        let value = $crate::serde_json::json!($($spec)+);
        let spec = $crate::Spec::from_yaml(value.to_string()).expect("invalid spec");
        spec
    }};
}

/// A generic, structured data object that can be turned into a recipe
/// when provided with the necessary option values
#[derive(Debug, Clone)]
pub struct SpecTemplate {
    name: Option<PkgNameBuf>,
    versions: HashSet<Version>,
    file_path: std::path::PathBuf,
    template: Arc<str>,
}

impl SpecTemplate {
    /// The complete source string for this template
    pub fn source(&self) -> &str {
        &self.template
    }

    /// The name of the item that this template will create.
    pub fn name(&self) -> Option<&PkgNameBuf> {
        self.name.as_ref()
    }

    /// The versions that are available to create with this template.
    ///
    /// An empty set does not signify no versions, but rather that
    /// nothing has been specified or discerned.
    pub fn versions(&self) -> &HashSet<Version> {
        &self.versions
    }

    /// Clear and reset the versions that are available to create
    /// with this template.
    pub fn set_versions(&mut self, versions: impl IntoIterator<Item = Version>) {
        self.versions.clear();
        self.versions.extend(versions);
    }
}

impl Template for SpecTemplate {
    fn file_path(&self) -> &Path {
        &self.file_path
    }

    fn render(&self, options: &OptionMap) -> Result<SpecFileData> {
        let data = super::TemplateData::new(options);
        let rendered = spk_schema_tera::render_template(
            self.file_path.to_string_lossy(),
            &self.template,
            &data,
        )
        .map_err(Error::InvalidTemplate)?;

        let file_data = SpecFileData::from_yaml(rendered)?;
        Ok(file_data)
    }
}

impl TemplateExt for SpecTemplate {
    fn from_file(path: &Path) -> Result<Self> {
        let file_path =
            dunce::canonicalize(path).map_err(|err| Error::InvalidPath(path.to_owned(), err))?;

        let file = std::fs::File::open(&file_path)
            .map_err(|err| Error::FileOpenError(file_path.to_owned(), err))?;

        let mut template = String::new();
        std::io::BufReader::new(file)
            .read_to_string(&mut template)
            .map_err(|err| Error::String(format!("Failed to read file {path:?}: {err}")))?;

        // validate that the template is still a valid yaml mapping even
        // though we will need to re-process it again later on
        let template_value: serde_yaml::Mapping = match serde_yaml::from_str(&template) {
            Err(err) => {
                return Err(Error::InvalidYaml(SerdeError::new(
                    template,
                    SerdeYamlError(err),
                )));
            }
            Ok(v) => v,
        };

        let api = template_value.get(serde_yaml::Value::String("api".to_string()));

        if api.is_none() {
            tracing::warn!(
                spec_file = %file_path.to_string_lossy(),
                "Spec file is missing the 'api' field, this may be an error in the future"
            );
            tracing::warn!(
                " > for package specs in the original spk format, add a 'api: v0/package' line"
            );
        }

        let name_field = match api {
            Some(serde_yaml::Value::String(api)) => {
                let field = api.split("/").nth(1).unwrap_or("pkg");
                if field == "package" { "pkg" } else { field }
            }
            Some(_) => "pkg",
            None => "pkg",
        };

        let name = if name_field == "requirements" {
            // This is Spec data and does not have a name, e.g. Requests(V0::Requirements)
            None
        } else {
            // Read the name from the name field, check it is a valid
            // string, and turn it into a PkgNameBuf
            let pkg = template_value
                .get(serde_yaml::Value::String(name_field.to_string()))
                .ok_or_else(|| {
                    crate::Error::String(format!(
                        "Missing '{name_field}' field in spec file: {file_path:?}"
                    ))
                })?;

            let pkg = pkg.as_str().ok_or_else(|| {
                crate::Error::String(format!(
                    "Invalid value for '{name_field}' field: expected string, got {pkg:?} in {file_path:?}"
                ))
            })?;

            // it should never be possible for split to return 0 results
            // but this trick avoids the use of unwrap
            Some(PkgNameBuf::from_str(pkg.split('/').next().unwrap_or(pkg))?)
        };

        Ok(Self {
            file_path,
            name,
            versions: Default::default(),
            template: template.into(),
        })
    }
}

/// Specifies some buildable object within the spk ecosystem.
///
/// All build-able types have a recipe representation
/// that can be serialized and deserialized from a human-written
/// file or machine-managed persistent storage.
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize)]
#[serde(tag = "api")]
#[enum_dispatch(Deprecate, DeprecateMut)]
pub enum SpecRecipe {
    #[serde(rename = "v0/package")]
    V0Package(super::v0::Spec<VersionIdent>),
    #[serde(rename = "v0/platform")]
    V0Platform(super::v0::Platform),
    #[serde(rename = "v1/platform")]
    V1Platform(super::v1::Platform),
}

macro_rules! each_variant {
    ($self:ident, $bind:ident, $for_each:stmt) => {
        each_variant!($self, $bind => { $for_each })
    };
    ($self:ident, $bind:ident => $for_each:tt) => {
        match $self {
            SpecRecipe::V0Package($bind) => $for_each,
            SpecRecipe::V0Platform($bind) => $for_each,
            SpecRecipe::V1Platform($bind) => $for_each,
        }
    };
}

impl SpecRecipe {
    /// Access the recipe's build options
    pub fn build_options(&self) -> Cow<'_, [Opt]> {
        each_variant!(self, r, r.build_options())
    }
}

impl Recipe for SpecRecipe {
    type Output = Spec;
    type Variant = SpecVariant;
    type Test = SpecTest;

    fn ident(&self) -> &VersionIdent {
        each_variant!(self, r, Recipe::ident(r))
    }

    fn build_digest<V>(&self, variant: &V) -> Result<BuildId>
    where
        V: Variant,
    {
        each_variant!(self, r, Recipe::build_digest(r, variant))
    }

    fn default_variants(&self, options: &OptionMap) -> Cow<'_, Vec<Self::Variant>> {
        each_variant!(
            self,
            r,
            Cow::Owned(
                // use into_owned instead of iter().cloned() in case it's
                // already an owned instance
                #[allow(clippy::unnecessary_to_owned)]
                r.default_variants(options)
                    .into_owned()
                    .into_iter()
                    .map(SpecVariant::V0)
                    .collect(),
            )
        )
    }

    fn resolve_options<V>(&self, variant: &V) -> Result<OptionMap>
    where
        V: Variant,
    {
        each_variant!(self, r, r.resolve_options(variant))
    }

    fn get_build_requirements<V>(&self, variant: &V) -> Result<Cow<'_, RequirementsList>>
    where
        V: Variant,
    {
        each_variant!(self, r, r.get_build_requirements(variant))
    }

    fn get_tests<V>(&self, stage: TestStage, variant: &V) -> Result<Vec<Self::Test>>
    where
        V: Variant,
    {
        each_variant!(
            self,
            r,
            Ok(r.get_tests(stage, variant)?
                .into_iter()
                .map(SpecTest::V0)
                .collect())
        )
    }

    fn generate_source_build(&self, root: &Path) -> Result<Self::Output> {
        each_variant!(self, r, r.generate_source_build(root).map(Spec::V0Package))
    }

    fn generate_binary_build<V, E, P>(&self, variant: &V, build_env: &E) -> Result<Self::Output>
    where
        V: InputVariant,
        E: BuildEnv<Package = P>,
        P: Package,
    {
        each_variant!(
            self,
            r,
            r.generate_binary_build(variant, build_env)
                .map(Spec::V0Package)
        )
    }

    fn metadata(&self) -> &Meta {
        each_variant!(self, r, r.metadata())
    }
}

impl HasVersion for SpecRecipe {
    fn version(&self) -> &Version {
        each_variant!(self, r, r.version())
    }
}

impl Named for SpecRecipe {
    fn name(&self) -> &PkgName {
        each_variant!(self, r, r.name())
    }
}

impl RuntimeEnvironment for SpecRecipe {
    fn runtime_environment(&self) -> &[crate::EnvOp] {
        each_variant!(self, r, r.runtime_environment())
    }
}

impl Versioned for SpecRecipe {
    fn compat(&self) -> &Compat {
        each_variant!(self, spec, spec.compat())
    }
}

impl FromYaml for SpecRecipe {
    fn from_yaml<S: Into<String>>(yaml: S) -> std::result::Result<Self, SerdeError> {
        let yaml = yaml.into();

        // unfortunately, serde does not have a derive mechanism which
        // would allow us to specify a default enum variant for when
        // the 'api' field does not exist in a spec. To do this properly
        // and still be able to maintain source location data for
        // yaml errors, we need to deserialize twice: once to get the
        // api version, and a second time to deserialize that version.
        // deserializing into a value and then using from_value
        // instead of using from_str twice will lose useful context
        // info if the parsing errors.

        // the name of this struct appears in error messages when the
        // root of the yaml doc is not a mapping, so we use something
        // fairly generic, eg: 'expected struct DataApiVersionMapping'
        let with_version = match serde_yaml::from_str::<DataApiVersionMapping>(&yaml) {
            // we cannot simply use map_err because we need the compiler
            // to understand that we only pass ownership of 'yaml' if
            // the function is returning
            Err(err) => {
                return Err(SerdeError::new(yaml, SerdeYamlError(err)));
            }
            Ok(m) => m,
        };

        match with_version.api {
            ApiVersion::V0Package => {
                let inner = serde_yaml::from_str(&yaml)
                    .map_err(|err| SerdeError::new(yaml, SerdeYamlError(err)))?;
                Ok(Self::V0Package(inner))
            }
            ApiVersion::V0Platform => {
                let inner = serde_yaml::from_str(&yaml)
                    .map_err(|err| SerdeError::new(yaml, SerdeYamlError(err)))?;
                Ok(Self::V0Platform(inner))
            }
            ApiVersion::V0Requirements => {
                // Reading a list of requests/requirements file is not
                // supported here. But it might be in future.
                unimplemented!()
            }
        }
    }
}

// Used during the initial parsing to determine what kind of data is in a file
#[derive(Deserialize)]
struct DataApiVersionMapping {
    #[serde(default = "ApiVersion::default")]
    api: ApiVersion,
}

/// Enum for the kinds of data in a spk yaml file
#[derive(Debug)]
pub enum SpecFileData {
    /// A package or platform recipe
    Recipe(Arc<SpecRecipe>),
    /// A list of requests
    Requests(v0::Requirements),
}

impl SpecFileData {
    /// Return a SpecRecipe from this SpecFileData. Errors if it is
    /// not SpecRecipe
    pub fn into_recipe(self) -> Result<Arc<SpecRecipe>> {
        match self {
            SpecFileData::Recipe(r) => Ok(r.to_owned()),
            SpecFileData::Requests(_) => {
                Err(Error::String(
                    "A package or platform recipe spec is required in this context. This is requests data."
                        .to_string(),
                )
                )
            }
        }
    }

    /// Return a list of requests/requirements from this SpecFileData.
    /// Errors if it is not a list of requests
    pub fn into_requests(self) -> Result<v0::Requirements> {
        match self {
            SpecFileData::Recipe(_) => {
                Err(Error::String(
                    "Requests data is required in this context. This is a package or platform recipe spec."
                        .to_string(),
                )
                )
            }
            SpecFileData::Requests(r) => Ok(r.to_owned()),
        }
    }

    pub fn from_yaml<S: Into<String>>(yaml: S) -> Result<SpecFileData> {
        let yaml = yaml.into();

        let value: serde_yaml::Value =
            serde_yaml::from_str(&yaml).map_err(Error::SpecEncodingError)?;

        // First work out what kind of data this is, based on the
        // DataApiVersionMapping value.
        let with_version = match serde_yaml::from_value::<DataApiVersionMapping>(value.clone()) {
            // we cannot simply use map_err because we need the compiler
            // to understand that we only pass ownership of 'yaml' if
            // the function is returning
            Err(err) => {
                return Err(SerdeError::new(yaml, SerdeYamlError(err)).into());
            }
            Ok(m) => m,
        };

        // Create the appropriate object from the parsed value
        let spec = match with_version.api {
            ApiVersion::V0Package => {
                let inner = serde_yaml::from_value(value)
                    .map_err(|err| SerdeError::new(yaml, SerdeYamlError(err)))?;
                SpecFileData::Recipe(Arc::new(SpecRecipe::V0Package(inner)))
            }
            ApiVersion::V0Platform => {
                let inner = serde_yaml::from_value(value)
                    .map_err(|err| SerdeError::new(yaml, SerdeYamlError(err)))?;
                SpecFileData::Recipe(Arc::new(SpecRecipe::V0Platform(inner)))
            }
            ApiVersion::V0Requirements => {
                let requests: v0::Requirements = serde_yaml::from_value(value)
                    .map_err(|err| SerdeError::new(yaml, SerdeYamlError(err)))?;
                SpecFileData::Requests(requests)
            }
        };
        Ok(spec)
    }
}

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub enum SpecVariant {
    V0(v0::Variant),
}

impl super::Variant for SpecVariant {
    fn name(&self) -> Option<&str> {
        match self {
            Self::V0(v) => v.name(),
        }
    }

    fn options(&self) -> Cow<'_, OptionMap> {
        match self {
            Self::V0(v) => v.options(),
        }
    }

    fn additional_requirements(&self) -> Cow<'_, RequirementsList> {
        match self {
            Self::V0(v) => v.additional_requirements(),
        }
    }
}

impl std::fmt::Display for SpecVariant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::V0(v) => v.fmt(f),
        }
    }
}

pub enum SpecTest {
    V0(v0::TestSpec),
}

impl Test for SpecTest {
    fn script(&self) -> String {
        match self {
            Self::V0(t) => t.script(),
        }
    }

    fn additional_requirements(&self) -> Vec<Request> {
        match self {
            Self::V0(t) => t.additional_requirements(),
        }
    }
}

/// Specifies some data object within the spk ecosystem.
///
/// All resolve-able types have a spec representation that can be serialized
/// and deserialized from a `Repository`.
#[derive(Debug, Clone, Hash, Eq, PartialEq, Ord, PartialOrd, Serialize)]
#[serde(tag = "api")]
#[enum_dispatch(Deprecate, DeprecateMut)]
pub enum Spec {
    #[serde(rename = "v0/package")]
    V0Package(super::v0::Spec<BuildIdent>),
}

impl Satisfy<PkgRequest> for Spec {
    fn check_satisfies_request(&self, request: &PkgRequest) -> Compatibility {
        match self {
            Spec::V0Package(r) => r.check_satisfies_request(request),
        }
    }
}

impl Satisfy<VarRequest> for Spec {
    fn check_satisfies_request(&self, request: &VarRequest) -> Compatibility {
        match self {
            Spec::V0Package(r) => r.check_satisfies_request(request),
        }
    }
}

impl HasVersion for Spec {
    fn version(&self) -> &Version {
        match self {
            Spec::V0Package(r) => r.version(),
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

impl RuntimeEnvironment for Spec {
    fn runtime_environment(&self) -> &[crate::EnvOp] {
        match self {
            Spec::V0Package(r) => r.runtime_environment(),
        }
    }
}

impl Versioned for Spec {
    fn compat(&self) -> &Compat {
        match self {
            Spec::V0Package(spec) => spec.compat(),
        }
    }
}

// enum_dispatch does not support associated types.
impl Package for Spec {
    type Package = Self;

    fn ident(&self) -> &BuildIdent {
        match self {
            Spec::V0Package(spec) => Package::ident(spec),
        }
    }

    fn metadata(&self) -> &crate::metadata::Meta {
        match self {
            Spec::V0Package(spec) => spec.metadata(),
        }
    }

    fn option_values(&self) -> OptionMap {
        match self {
            Spec::V0Package(spec) => spec.option_values(),
        }
    }

    fn matches_all_filters(&self, filter_by: &Option<Vec<OptFilter>>) -> bool {
        match self {
            Spec::V0Package(spec) => spec.matches_all_filters(filter_by),
        }
    }

    fn sources(&self) -> &Vec<super::SourceSpec> {
        match self {
            Spec::V0Package(spec) => spec.sources(),
        }
    }

    fn embedded(&self) -> &super::EmbeddedPackagesList {
        match self {
            Spec::V0Package(spec) => spec.embedded(),
        }
    }

    fn embedded_as_packages(
        &self,
    ) -> std::result::Result<Vec<(Self::Package, Option<Component>)>, &str> {
        match self {
            Spec::V0Package(spec) => spec
                .embedded_as_packages()
                .map(|vec| vec.into_iter().map(|(r, c)| (r.into(), c)).collect()),
        }
    }

    fn components(&self) -> &super::ComponentSpecList {
        match self {
            Spec::V0Package(spec) => spec.components(),
        }
    }

    fn get_build_options(&self) -> &Vec<Opt> {
        match self {
            Spec::V0Package(spec) => spec.get_build_options(),
        }
    }

    fn get_build_requirements(&self) -> crate::Result<Cow<'_, RequirementsList>> {
        match self {
            Spec::V0Package(spec) => spec.get_build_requirements(),
        }
    }

    fn runtime_requirements(&self) -> Cow<'_, crate::RequirementsList> {
        match self {
            Spec::V0Package(spec) => spec.runtime_requirements(),
        }
    }

    fn validation(&self) -> &super::ValidationSpec {
        match self {
            Spec::V0Package(spec) => spec.validation(),
        }
    }

    fn build_script(&self) -> String {
        match self {
            Spec::V0Package(spec) => spec.build_script(),
        }
    }

    fn downstream_build_requirements<'a>(
        &self,
        components: impl IntoIterator<Item = &'a Component>,
    ) -> Cow<'_, crate::RequirementsList> {
        match self {
            Spec::V0Package(spec) => spec.downstream_build_requirements(components),
        }
    }

    fn downstream_runtime_requirements<'a>(
        &self,
        components: impl IntoIterator<Item = &'a Component>,
    ) -> Cow<'_, crate::RequirementsList> {
        match self {
            Spec::V0Package(spec) => spec.downstream_runtime_requirements(components),
        }
    }
}

impl PackageMut for Spec {
    fn set_build(&mut self, build: Build) {
        match self {
            Spec::V0Package(spec) => spec.set_build(build),
        }
    }
}

impl FromYaml for Spec {
    fn from_yaml<S: Into<String>>(yaml: S) -> std::result::Result<Self, SerdeError> {
        let yaml = yaml.into();

        // unfortunately, serde does not have a derive mechanism which
        // would allow us to specify a default enum variant for when
        // the 'api' field does not exist in a spec. To do this properly
        // and still be able to maintain source location data for
        // yaml errors, we need to deserialize twice: once to get the
        // api version, and a second time to deserialize that version.
        // deserializing into a value and then using from_value
        // instead of using from_str twice will lose useful context
        // info if the parsing errors.

        // the name of this struct appears in error messages when the
        // root of the yaml doc is not a mapping, so we use something
        // fairly generic, eg: 'expected struct DataApiVersionMapping'
        let with_version = match serde_yaml::from_str::<DataApiVersionMapping>(&yaml) {
            // we cannot simply use map_err because we need the compiler
            // to understand that we only pass ownership of 'yaml' if
            // the function is returning
            Err(err) => {
                return Err(SerdeError::new(yaml, SerdeYamlError(err)));
            }
            Ok(m) => m,
        };

        match with_version.api {
            ApiVersion::V0Package => {
                let inner = serde_yaml::from_str(&yaml)
                    .map_err(|err| SerdeError::new(yaml, SerdeYamlError(err)))?;
                Ok(Self::V0Package(inner))
            }
            ApiVersion::V0Platform => {
                let inner = serde_yaml::from_str(&yaml)
                    .map_err(|err| SerdeError::new(yaml, SerdeYamlError(err)))?;
                Ok(Self::V0Package(inner))
            }
            ApiVersion::V0Requirements => {
                // Reading a list of requests/requirement file is not
                // supported here. But it might be in future.
                unimplemented!()
            }
        }
    }
}

impl AsRef<Spec> for Spec {
    fn as_ref(&self) -> &Spec {
        self
    }
}

#[derive(Deserialize, Serialize, Copy, Clone)]
pub enum ApiVersion {
    #[serde(rename = "v0/package")]
    V0Package,
    #[serde(rename = "v0/platform")]
    V0Platform,
    #[serde(rename = "v0/requirements")]
    V0Requirements,
}

impl Default for ApiVersion {
    fn default() -> Self {
        Self::V0Package
    }
}
