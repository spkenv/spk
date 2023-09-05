// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::borrow::Cow;
use std::path::Path;

use spk_schema_ident::VersionIdent;

use crate::foundation::option_map::OptionMap;
use crate::foundation::spec_ops::{Named, Versioned};
use crate::{InputVariant, Package, RequirementsList, Result, TestStage, Variant};

/// Return the resolved packages from a solution.
pub trait BuildEnv {
    type Package: super::Package;

    fn build_env(&self) -> Vec<Self::Package>;
}

/// Can be used to build a package.
#[enum_dispatch::enum_dispatch]
pub trait Recipe:
    Named + Versioned + super::Deprecate + Clone + Eq + std::hash::Hash + Sync + Send
{
    type Output: super::Package;
    type Variant: super::Variant + Clone;
    type Test: super::Test;

    /// Build an identifier to represent this recipe.
    ///
    /// The returned identifier will never have an associated build.
    fn ident(&self) -> &VersionIdent;

    /// Return the default variants defined in this recipe.
    fn default_variants(&self) -> Cow<'_, Vec<Self::Variant>>;

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
    fn get_build_requirements<V>(&self, variant: &V) -> Result<Cow<'_, RequirementsList>>
    where
        V: Variant;

    /// Return the tests defined for this package at the given stage.
    fn get_tests<V>(&self, stage: TestStage, variant: &V) -> Result<Vec<Self::Test>>
    where
        V: Variant;

    /// Create a new source package from this recipe and the given parameters.
    fn generate_source_build(&self, root: &Path) -> Result<Self::Output>;

    /// Create a new binary package from this recipe and the given parameters.
    fn generate_binary_build<V, E, P>(&self, variant: &V, build_env: &E) -> Result<Self::Output>
    where
        V: InputVariant,
        E: BuildEnv<Package = P>,
        P: Package;
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

    fn default_variants(&self) -> Cow<'_, Vec<Self::Variant>> {
        (**self).default_variants()
    }

    fn resolve_options<V>(&self, variant: &V) -> Result<OptionMap>
    where
        V: Variant,
    {
        (**self).resolve_options(variant)
    }

    fn get_build_requirements<V>(&self, variant: &V) -> Result<Cow<'_, RequirementsList>>
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

    fn generate_binary_build<V, E, P>(&self, variant: &V, build_env: &E) -> Result<Self::Output>
    where
        V: InputVariant,
        E: BuildEnv<Package = P>,
        P: Package,
    {
        (**self).generate_binary_build(variant, build_env)
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

    fn default_variants(&self) -> Cow<'_, Vec<Self::Variant>> {
        (**self).default_variants()
    }

    fn resolve_options<V>(&self, variant: &V) -> Result<OptionMap>
    where
        V: Variant,
    {
        (**self).resolve_options(variant)
    }

    fn get_build_requirements<V>(&self, variant: &V) -> Result<Cow<'_, RequirementsList>>
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

    fn generate_binary_build<V, E, P>(&self, variant: &V, build_env: &E) -> Result<Self::Output>
    where
        V: InputVariant,
        E: BuildEnv<Package = P>,
        P: Package,
    {
        (**self).generate_binary_build(variant, build_env)
    }
}
