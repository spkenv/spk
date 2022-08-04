// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::path::Path;

use spk_schema_ident::VersionIdent;

use crate::foundation::option_map::OptionMap;
use crate::foundation::spec_ops::{Named, Versioned};
use crate::ident::Request;
use crate::test_spec::TestSpec;
use crate::{Package, Result};

/// Return the resolved packages from a solution.
pub trait BuildEnv {
    type Package: super::Package;

    fn build_env(&self) -> Vec<Self::Package>;
}

/// Identifies which variant (or the default build) is to be
/// built.
#[derive(Clone, Debug, Hash)]
pub enum BuildVariant {
    /// Build with the defaults rather than a variant.
    Default,
    /// Build a particular variant by index.
    Variant(usize),
}

/// Can be used to build a package.
#[enum_dispatch::enum_dispatch]
pub trait Recipe:
    Named + Versioned + super::Deprecate + Clone + Eq + std::hash::Hash + Sync + Send
{
    type Output: super::Package;

    /// Build an identifier to represent this recipe.
    ///
    /// The returned identifier will never have an associated build.
    fn ident(&self) -> &VersionIdent;

    /// Return the default variants to be built for this recipe
    fn default_variants(&self) -> Vec<BuildVariant>;

    /// Produce the full set of build options given the inputs.
    ///
    /// The returned option map will include any values from the inputs
    /// that are relevant to this recipe with the addition of any missing
    /// default values. Any issues or invalid inputs results in an error.
    fn resolve_options(
        &self,
        build_variant: &BuildVariant,
        inputs: &OptionMap,
    ) -> Result<OptionMap>;

    /// Identify the requirements for a build of this recipe.
    fn get_build_requirements(&self, options: &OptionMap) -> Result<Vec<Request>>;

    /// Return the tests defined for this package.
    fn get_tests(&self, options: &OptionMap) -> Result<Vec<TestSpec>>;

    /// Create a new source package from this recipe and the given parameters.
    fn generate_source_build(&self, root: &Path) -> Result<Self::Output>;

    /// Create a new binary package from this recipe and the given parameters.
    fn generate_binary_build<E, P>(
        &self,
        build_variant: &BuildVariant,
        options: &OptionMap,
        build_env: &E,
    ) -> Result<Self::Output>
    where
        E: BuildEnv<Package = P>,
        P: Package;
}

impl<T> Recipe for std::sync::Arc<T>
where
    T: Recipe,
{
    type Output = T::Output;

    fn ident(&self) -> &VersionIdent {
        (**self).ident()
    }

    fn default_variants(&self) -> Vec<BuildVariant> {
        (**self).default_variants()
    }

    fn resolve_options(
        &self,
        build_variant: &BuildVariant,
        inputs: &OptionMap,
    ) -> Result<OptionMap> {
        (**self).resolve_options(build_variant, inputs)
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
        build_variant: &BuildVariant,
        options: &OptionMap,
        build_env: &E,
    ) -> Result<Self::Output>
    where
        E: BuildEnv<Package = P>,
        P: Package,
    {
        (**self).generate_binary_build(build_variant, options, build_env)
    }
}

impl<T> Recipe for &T
where
    T: Recipe,
{
    type Output = T::Output;

    fn ident(&self) -> &VersionIdent {
        (**self).ident()
    }

    fn default_variants(&self) -> Vec<BuildVariant> {
        (**self).default_variants()
    }

    fn resolve_options(
        &self,
        build_variant: &BuildVariant,
        inputs: &OptionMap,
    ) -> Result<OptionMap> {
        (**self).resolve_options(build_variant, inputs)
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
        build_variant: &BuildVariant,
        options: &OptionMap,
        build_env: &E,
    ) -> Result<Self::Output>
    where
        E: BuildEnv<Package = P>,
        P: Package,
    {
        (**self).generate_binary_build(build_variant, options, build_env)
    }
}
