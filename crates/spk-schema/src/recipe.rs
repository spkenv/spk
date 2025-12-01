// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::borrow::Cow;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use spk_schema_foundation::ident::{RequestWithOptions, VersionIdent};

use crate::foundation::ident_build::BuildId;
use crate::foundation::option_map::OptionMap;
use crate::foundation::spec_ops::{Named, Versioned};
use crate::metadata::Meta;
use crate::{InputVariant, Package, RequirementsList, Result, TestStage, Variant};

/// Return the resolved packages from a solution.
pub trait BuildEnv {
    type Package: super::Package;

    fn build_env(&self) -> Vec<Self::Package>;

    fn env_vars(&self) -> HashMap<String, String>;
}

/// An empty build environment implementation, primarily used in testing
impl BuildEnv for () {
    type Package = crate::Spec;

    fn build_env(&self) -> Vec<Self::Package> {
        Vec::new()
    }

    fn env_vars(&self) -> HashMap<String, String> {
        HashMap::default()
    }
}

/// Can be used to build a package.
#[enum_dispatch::enum_dispatch]
pub trait Recipe:
    Named
    + Versioned
    + super::Deprecate
    + crate::RuntimeEnvironment
    + Clone
    + Eq
    + std::hash::Hash
    + Sync
    + Send
{
    type Output: super::Package;
    type Variant: super::Variant + Clone;
    type Test: super::Test;

    /// Build an identifier to represent this recipe.
    ///
    /// The returned identifier will never have an associated build.
    fn ident(&self) -> &VersionIdent;

    /// Calculate the build digest that would be produced by building this
    /// recipe with the given options.
    fn build_digest<V>(&self, variant: &V) -> Result<BuildId>
    where
        V: Variant;

    /// Return the build script for building this recipe.
    fn build_script(&self) -> String;

    /// Return the default variants defined in this recipe.
    ///
    /// The prevailing option overrides are needed to return a correct default
    /// variant, including host options if they are enabled.
    fn default_variants(&self, options: &OptionMap) -> Cow<'_, Vec<Self::Variant>>;

    /// Produce the full set of build options given the inputs.
    ///
    /// The returned option map will include any values from the inputs
    /// that are relevant to this recipe with the addition of any missing
    /// default values. Any issues or invalid inputs results in an error.
    fn resolve_options<V>(&self, variant: &V) -> Result<OptionMap>
    where
        V: Variant;

    /// Identify the requirements for a build of this recipe.
    ///
    /// This should also validate and include the items specified
    /// by [`Variant::additional_requirements`].
    fn get_build_requirements<V>(
        &self,
        variant: &V,
    ) -> Result<Cow<'_, RequirementsList<RequestWithOptions>>>
    where
        V: Variant;

    /// Return the tests defined for this package at the given stage.
    fn get_tests<V>(&self, stage: TestStage, variant: &V) -> Result<Vec<Self::Test>>
    where
        V: Variant;

    /// Create a new source package from this recipe and the given parameters.
    fn generate_source_build(&self, root: &Path) -> Result<Self::Output>;

    /// Create a new binary package from this recipe and the given parameters.
    fn generate_binary_build<V, E, P>(self, variant: &V, build_env: &E) -> Result<Self::Output>
    where
        V: InputVariant,
        E: BuildEnv<Package = P>,
        P: Package;

    /// Return the metadata for this package.
    fn metadata(&self) -> &Meta;

    /// Return the set of configured validators when building this package
    fn validation(&self) -> &super::ValidationSpec;
}

impl<T> Recipe for std::sync::Arc<T>
where
    T: Recipe,
{
    type Output = T::Output;
    type Variant = T::Variant;
    type Test = T::Test;

    fn ident(&self) -> &VersionIdent {
        (**self).ident()
    }

    fn build_digest<V>(&self, variant: &V) -> Result<BuildId>
    where
        V: Variant,
    {
        (**self).build_digest(variant)
    }

    fn build_script(&self) -> String {
        (**self).build_script()
    }

    fn default_variants(&self, options: &OptionMap) -> Cow<'_, Vec<Self::Variant>> {
        (**self).default_variants(options)
    }

    fn resolve_options<V>(&self, variant: &V) -> Result<OptionMap>
    where
        V: Variant,
    {
        (**self).resolve_options(variant)
    }

    fn get_build_requirements<V>(
        &self,
        variant: &V,
    ) -> Result<Cow<'_, RequirementsList<RequestWithOptions>>>
    where
        V: Variant,
    {
        (**self).get_build_requirements(variant)
    }

    fn get_tests<V>(&self, stage: TestStage, variant: &V) -> Result<Vec<Self::Test>>
    where
        V: Variant,
    {
        (**self).get_tests(stage, variant)
    }

    fn generate_source_build(&self, root: &Path) -> Result<Self::Output> {
        (**self).generate_source_build(root)
    }

    fn generate_binary_build<V, E, P>(self, variant: &V, build_env: &E) -> Result<Self::Output>
    where
        V: InputVariant,
        E: BuildEnv<Package = P>,
        P: Package,
    {
        Arc::unwrap_or_clone(self).generate_binary_build(variant, build_env)
    }

    fn metadata(&self) -> &Meta {
        (**self).metadata()
    }

    fn validation(&self) -> &super::ValidationSpec {
        (**self).validation()
    }
}

impl<T> Recipe for &T
where
    T: Recipe,
{
    type Output = T::Output;
    type Variant = T::Variant;
    type Test = T::Test;

    fn ident(&self) -> &VersionIdent {
        (**self).ident()
    }

    fn build_digest<V>(&self, variant: &V) -> Result<BuildId>
    where
        V: Variant,
    {
        (**self).build_digest(variant)
    }

    fn build_script(&self) -> String {
        (**self).build_script()
    }

    fn default_variants(&self, options: &OptionMap) -> Cow<'_, Vec<Self::Variant>> {
        (**self).default_variants(options)
    }

    fn resolve_options<V>(&self, variant: &V) -> Result<OptionMap>
    where
        V: Variant,
    {
        (**self).resolve_options(variant)
    }

    fn get_build_requirements<V>(
        &self,
        variant: &V,
    ) -> Result<Cow<'_, RequirementsList<RequestWithOptions>>>
    where
        V: Variant,
    {
        (**self).get_build_requirements(variant)
    }

    fn get_tests<V>(&self, stage: TestStage, variant: &V) -> Result<Vec<Self::Test>>
    where
        V: Variant,
    {
        (**self).get_tests(stage, variant)
    }

    fn generate_source_build(&self, root: &Path) -> Result<Self::Output> {
        (**self).generate_source_build(root)
    }

    fn generate_binary_build<V, E, P>(self, variant: &V, build_env: &E) -> Result<Self::Output>
    where
        V: InputVariant,
        E: BuildEnv<Package = P>,
        P: Package,
    {
        self.clone().generate_binary_build(variant, build_env)
    }

    fn metadata(&self) -> &Meta {
        (**self).metadata()
    }

    fn validation(&self) -> &super::ValidationSpec {
        (**self).validation()
    }
}
