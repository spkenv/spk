// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::sync::Arc;

use crate::Result;

/// Some item that has an associated package name
#[enum_dispatch::enum_dispatch]
pub trait Versioned {
    /// The associated version number
    fn version(&self) -> &super::Version;
}

impl<T: Versioned> Versioned for Arc<T> {
    fn version(&self) -> &super::Version {
        (**self).version()
    }
}

impl<T: Versioned> Versioned for &T {
    fn version(&self) -> &super::Version {
        (**self).version()
    }
}

/// Can be used to build a package.
#[enum_dispatch::enum_dispatch]
pub trait Recipe: super::Named + Versioned + super::Deprecate {
    type Output: super::Package;

    /// Build an identifier to represent this recipe.
    ///
    /// The returned identifier will not have an associated build.
    fn ident(&self) -> super::Ident {
        super::Ident {
            name: self.name().to_owned(),
            version: self.version().clone(),
            build: None,
        }
    }

    /// Return the default variants to be built for this recipe
    fn default_variants(&self) -> &Vec<super::OptionMap>;

    /// Produce the full set of build options given the inputs.
    ///
    /// The returned option map will include any values from the inputs
    /// that are relevant to this recipe with the addition of any missing
    /// default values. Any issues or invalid inputs results in an error.
    fn resolve_options(&self, inputs: &super::OptionMap) -> Result<super::OptionMap>;

    /// Identify the requirements for a build of this recipe.
    fn get_build_requirements(&self, options: &super::OptionMap) -> Result<Vec<super::Request>>;

    /// Return the tests defined for this package.
    fn get_tests(&self, options: &super::OptionMap) -> Result<Vec<super::TestSpec>>;

    /// Create a new source package from this recipe and the given parameters.
    fn generate_source_build(&self) -> Result<Self::Output>;

    /// Create a new binary package from this recipe and the given parameters.
    fn generate_binary_build(
        &self,
        options: &super::OptionMap,
        build_env: &crate::solve::Solution,
    ) -> Result<Self::Output>;
}

impl<T> Recipe for std::sync::Arc<T>
where
    T: Recipe,
{
    type Output = T::Output;

    fn ident(&self) -> super::Ident {
        (**self).ident()
    }

    fn default_variants(&self) -> &Vec<super::OptionMap> {
        (**self).default_variants()
    }

    fn resolve_options(&self, inputs: &super::OptionMap) -> Result<super::OptionMap> {
        (**self).resolve_options(inputs)
    }

    fn get_build_requirements(&self, options: &super::OptionMap) -> Result<Vec<super::Request>> {
        (**self).get_build_requirements(options)
    }

    fn get_tests(&self, options: &super::OptionMap) -> Result<Vec<super::TestSpec>> {
        (**self).get_tests(options)
    }

    fn generate_source_build(&self) -> Result<Self::Output> {
        (**self).generate_source_build()
    }

    fn generate_binary_build(
        &self,
        options: &super::OptionMap,
        build_env: &crate::solve::Solution,
    ) -> Result<Self::Output> {
        (**self).generate_binary_build(options, build_env)
    }
}

impl<T> Recipe for &T
where
    T: Recipe,
{
    type Output = T::Output;

    fn ident(&self) -> super::Ident {
        (**self).ident()
    }

    fn default_variants(&self) -> &Vec<super::OptionMap> {
        (**self).default_variants()
    }

    fn resolve_options(&self, inputs: &super::OptionMap) -> Result<super::OptionMap> {
        (**self).resolve_options(inputs)
    }

    fn get_build_requirements(&self, options: &super::OptionMap) -> Result<Vec<super::Request>> {
        (**self).get_build_requirements(options)
    }

    fn get_tests(&self, options: &super::OptionMap) -> Result<Vec<super::TestSpec>> {
        (**self).get_tests(options)
    }

    fn generate_source_build(&self) -> Result<Self::Output> {
        (**self).generate_source_build()
    }

    fn generate_binary_build(
        &self,
        options: &super::OptionMap,
        build_env: &crate::solve::Solution,
    ) -> Result<Self::Output> {
        (**self).generate_binary_build(options, build_env)
    }
}
