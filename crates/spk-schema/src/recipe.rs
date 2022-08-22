// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::path::Path;

use crate::foundation::option_map::OptionMap;
use crate::foundation::spec_ops::{Named, RecipeOps, Versioned};
use crate::ident::{Ident, Request};
use crate::{test_spec::TestSpec, Package, Result};

/// Return the resolved packages from a solution.
pub trait BuildEnv {
    type Package: super::Package;

    fn build_env(&self) -> Vec<Self::Package>;
}

/// Can be used to build a package.
#[enum_dispatch::enum_dispatch]
pub trait Recipe:
    RecipeOps + Named + Versioned + super::Deprecate + Clone + Eq + std::hash::Hash + Sync + Send
{
    type Output: super::Package;

    /// Return the default variants to be built for this recipe
    fn default_variants(&self) -> &Vec<OptionMap>;

    /// Produce the full set of build options given the inputs.
    ///
    /// The returned option map will include any values from the inputs
    /// that are relevant to this recipe with the addition of any missing
    /// default values. Any issues or invalid inputs results in an error.
    fn resolve_options(&self, inputs: &OptionMap) -> Result<OptionMap>;

    /// Identify the requirements for a build of this recipe.
    fn get_build_requirements(&self, options: &OptionMap) -> Result<Vec<Request>>;

    /// Return the tests defined for this package.
    fn get_tests(&self, options: &OptionMap) -> Result<Vec<TestSpec>>;

    /// Create a new source package from this recipe and the given parameters.
    fn generate_source_build(&self, root: &Path) -> Result<Self::Output>;

    /// Create a new binary package from this recipe and the given parameters.
    fn generate_binary_build<E, P>(
        &self,
        options: &OptionMap,
        build_env: &E,
    ) -> Result<Self::Output>
    where
        E: BuildEnv<Package = P>,
        P: Package<Ident = Ident>;
}

impl<T> Recipe for std::sync::Arc<T>
where
    T: Recipe,
{
    type Output = T::Output;

    fn default_variants(&self) -> &Vec<OptionMap> {
        (**self).default_variants()
    }

    fn resolve_options(&self, inputs: &OptionMap) -> Result<OptionMap> {
        (**self).resolve_options(inputs)
    }

    fn get_build_requirements(&self, options: &OptionMap) -> Result<Vec<Request>> {
        (**self).get_build_requirements(options)
    }

    fn get_tests(&self, options: &OptionMap) -> Result<Vec<TestSpec>> {
        (**self).get_tests(options)
    }

    fn generate_source_build(&self, root: &Path) -> Result<Self::Output> {
        (**self).generate_source_build(root)
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
        (**self).generate_binary_build(options, build_env)
    }
}

impl<T> Recipe for &T
where
    T: Recipe,
{
    type Output = T::Output;

    fn default_variants(&self) -> &Vec<OptionMap> {
        (**self).default_variants()
    }

    fn resolve_options(&self, inputs: &OptionMap) -> Result<OptionMap> {
        (**self).resolve_options(inputs)
    }

    fn get_build_requirements(&self, options: &OptionMap) -> Result<Vec<Request>> {
        (**self).get_build_requirements(options)
    }

    fn get_tests(&self, options: &OptionMap) -> Result<Vec<TestSpec>> {
        (**self).get_tests(options)
    }

    fn generate_source_build(&self, root: &Path) -> Result<Self::Output> {
        (**self).generate_source_build(root)
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
        (**self).generate_binary_build(options, build_env)
    }
}
