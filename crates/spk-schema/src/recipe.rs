// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::borrow::Cow;
use std::path::Path;

use spk_schema_ident::{PkgRequest, Satisfy, VersionIdent};

use crate::foundation::option_map::OptionMap;
use crate::foundation::spec_ops::{Named, Versioned};
use crate::{BuildEnv, Package, RequirementsList, Result, TestStage};

/// Can be used to build a package.
#[enum_dispatch::enum_dispatch]
pub trait Recipe:
    Named + Versioned + super::Deprecate + Clone + Eq + std::hash::Hash + Sync + Send
{
    type Output: super::Package;
    type Test: super::Test;
    type Variant: super::Variant + Clone;

    /// Build an identifier to represent this recipe.
    ///
    /// The returned identifier will never have an associated build.
    fn ident(&self) -> &VersionIdent;

    /// Return the default variants to be built for this recipe
    fn default_variants(&self) -> Cow<'_, Vec<Self::Variant>>;

    /// Produce the full set of build options given the inputs.
    ///
    /// The returned option map will include any values from the inputs
    /// that are relevant to this recipe with the addition of any missing
    /// default values. Any issues or invalid inputs results in an error.
    fn resolve_options(&self, inputs: &OptionMap) -> Result<OptionMap>;

    /// Identify the requirements for a build of this recipe.
    fn get_build_requirements(&self, options: &OptionMap) -> Result<Cow<'_, RequirementsList>>;

    /// Return the tests defined for this package at the given stage.
    fn get_tests(&self, stage: TestStage, options: &OptionMap) -> Result<Vec<Self::Test>>;

    /// Create a new source package from this recipe and the given parameters.
    fn generate_source_build(&self, root: &Path) -> Result<Self::Output>;

    /// Create a new binary package from this recipe and the given parameters.
    fn generate_binary_build<E, P>(&self, build_env: &E) -> Result<Self::Output>
    where
        E: BuildEnv<Package = P>,
        P: Package + Satisfy<PkgRequest>;
}

impl<T> Recipe for std::sync::Arc<T>
where
    T: Recipe,
{
    type Output = T::Output;
    type Test = T::Test;
    type Variant = T::Variant;

    fn ident(&self) -> &VersionIdent {
        (**self).ident()
    }

    fn default_variants(&self) -> Cow<'_, Vec<Self::Variant>> {
        (**self).default_variants()
    }

    fn resolve_options(&self, inputs: &OptionMap) -> Result<OptionMap> {
        (**self).resolve_options(inputs)
    }

    fn get_build_requirements(&self, options: &OptionMap) -> Result<Cow<'_, RequirementsList>> {
        (**self).get_build_requirements(options)
    }

    fn get_tests(&self, stage: TestStage, options: &OptionMap) -> Result<Vec<Self::Test>> {
        (**self).get_tests(stage, options)
    }

    fn generate_source_build(&self, root: &Path) -> Result<Self::Output> {
        (**self).generate_source_build(root)
    }

    fn generate_binary_build<E, P>(&self, build_env: &E) -> Result<Self::Output>
    where
        E: BuildEnv<Package = P>,
        P: Package + Satisfy<PkgRequest>,
    {
        (**self).generate_binary_build(build_env)
    }
}

impl<T> Recipe for &T
where
    T: Recipe,
{
    type Output = T::Output;
    type Test = T::Test;
    type Variant = T::Variant;

    fn ident(&self) -> &VersionIdent {
        (**self).ident()
    }

    fn default_variants(&self) -> Cow<'_, Vec<Self::Variant>> {
        (**self).default_variants()
    }

    fn resolve_options(&self, inputs: &OptionMap) -> Result<OptionMap> {
        (**self).resolve_options(inputs)
    }

    fn get_build_requirements(&self, options: &OptionMap) -> Result<Cow<'_, RequirementsList>> {
        (**self).get_build_requirements(options)
    }

    fn get_tests(&self, stage: TestStage, options: &OptionMap) -> Result<Vec<Self::Test>> {
        (**self).get_tests(stage, options)
    }

    fn generate_source_build(&self, root: &Path) -> Result<Self::Output> {
        (**self).generate_source_build(root)
    }

    fn generate_binary_build<E, P>(&self, build_env: &E) -> Result<Self::Output>
    where
        E: BuildEnv<Package = P>,
        P: Package + Satisfy<PkgRequest>,
    {
        (**self).generate_binary_build(build_env)
    }
}
