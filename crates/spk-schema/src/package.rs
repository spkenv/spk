// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::borrow::Cow;

use spk_schema_foundation::ident_build::Build;
use spk_schema_foundation::spec_ops::{Named, Versioned};
use spk_schema_ident::BuildIdent;

use super::RequirementsList;
use crate::foundation::ident_component::Component;
use crate::foundation::option_map::OptionMap;
use crate::foundation::version::Compatibility;
use crate::DeprecateMut;

#[cfg(test)]
#[path = "./package_test.rs"]
mod package_test;

/// Can be resolved into an environment.
#[enum_dispatch::enum_dispatch]
pub trait Package:
    Named + Versioned + super::Deprecate + Clone + Eq + std::hash::Hash + Sync + Send
{
    type Package;

    /// The full identifier for this package
    ///
    /// This includes the version and optional build
    fn ident(&self) -> &BuildIdent;

    /// The values for this packages options used for this build.
    fn option_values(&self) -> OptionMap;

    /// Return the location of sources for this package
    fn sources(&self) -> &Vec<super::SourceSpec>;

    /// The packages that are embedded within this one
    fn embedded(&self) -> &super::EmbeddedPackagesList;

    /// The packages that are embedded within this one.
    ///
    /// Return both top-level embedded packages and packages that are
    /// embedded inside a component. The returned list is a pair of the
    /// embedded package and the component it came from, if any.
    #[allow(clippy::type_complexity)]
    fn embedded_as_packages(
        &self,
    ) -> std::result::Result<Vec<(Self::Package, Option<Component>)>, &str>;

    /// The components defined by this package
    fn components(&self) -> &super::ComponentSpecList;

    /// The set of operations to perform on the environment when running this package
    fn runtime_environment(&self) -> &Vec<super::EnvOp>;

    /// Identify the requirements for a build of this package.
    fn get_build_requirements(&self) -> crate::Result<Cow<'_, RequirementsList>>;

    /// Requests that must be met to use this package
    fn runtime_requirements(&self) -> Cow<'_, RequirementsList>;

    /// Requests that must be satisfied by the build
    /// environment of any package built against this one
    ///
    /// These requirements are not injected downstream, instead
    /// they need to be present in the downstream package itself
    fn downstream_build_requirements<'a>(
        &self,
        components: impl IntoIterator<Item = &'a Component>,
    ) -> Cow<'_, RequirementsList>;

    /// Requests that must be satisfied by the runtime
    /// environment of any package built against this one
    ///
    /// These requirements are not injected downstream, instead
    /// they need to be present in the downstream package itself
    fn downstream_runtime_requirements<'a>(
        &self,
        components: impl IntoIterator<Item = &'a Component>,
    ) -> Cow<'_, RequirementsList>;

    /// Return the set of configured validators when building this package
    fn validation(&self) -> &super::ValidationSpec;

    /// Return the build script for building package
    fn build_script(&self) -> String;

    /// Validate the given options against the options in this spec.
    fn validate_options(&self, given_options: &OptionMap) -> Compatibility;
}

pub trait PackageMut: Package + DeprecateMut {
    /// Modify the build identifier for this package
    fn set_build(&mut self, build: Build);
}

impl<T: Package + Send + Sync> Package for std::sync::Arc<T> {
    type Package = T::Package;

    fn ident(&self) -> &BuildIdent {
        (**self).ident()
    }

    fn option_values(&self) -> OptionMap {
        (**self).option_values()
    }

    fn sources(&self) -> &Vec<super::SourceSpec> {
        (**self).sources()
    }

    fn embedded(&self) -> &super::EmbeddedPackagesList {
        (**self).embedded()
    }

    fn embedded_as_packages(
        &self,
    ) -> std::result::Result<Vec<(Self::Package, Option<Component>)>, &str> {
        (**self).embedded_as_packages()
    }

    fn components(&self) -> &super::ComponentSpecList {
        (**self).components()
    }

    fn runtime_environment(&self) -> &Vec<super::EnvOp> {
        (**self).runtime_environment()
    }

    fn get_build_requirements(&self) -> crate::Result<Cow<'_, RequirementsList>> {
        (**self).get_build_requirements()
    }

    fn runtime_requirements(&self) -> Cow<'_, RequirementsList> {
        (**self).runtime_requirements()
    }

    fn downstream_build_requirements<'a>(
        &self,
        components: impl IntoIterator<Item = &'a Component>,
    ) -> Cow<'_, RequirementsList> {
        (**self).downstream_build_requirements(components)
    }

    fn downstream_runtime_requirements<'a>(
        &self,
        components: impl IntoIterator<Item = &'a Component>,
    ) -> Cow<'_, RequirementsList> {
        (**self).downstream_build_requirements(components)
    }

    fn validation(&self) -> &super::ValidationSpec {
        (**self).validation()
    }

    fn build_script(&self) -> String {
        (**self).build_script()
    }

    fn validate_options(&self, given_options: &OptionMap) -> Compatibility {
        (**self).validate_options(given_options)
    }
}

impl<T: Package + Send + Sync> Package for Box<T> {
    type Package = T::Package;

    fn ident(&self) -> &BuildIdent {
        (**self).ident()
    }

    fn option_values(&self) -> OptionMap {
        (**self).option_values()
    }

    fn sources(&self) -> &Vec<super::SourceSpec> {
        (**self).sources()
    }

    fn embedded(&self) -> &super::EmbeddedPackagesList {
        (**self).embedded()
    }

    fn embedded_as_packages(
        &self,
    ) -> std::result::Result<Vec<(Self::Package, Option<Component>)>, &str> {
        (**self).embedded_as_packages()
    }

    fn components(&self) -> &super::ComponentSpecList {
        (**self).components()
    }

    fn runtime_environment(&self) -> &Vec<super::EnvOp> {
        (**self).runtime_environment()
    }

    fn get_build_requirements(&self) -> crate::Result<Cow<'_, RequirementsList>> {
        (**self).get_build_requirements()
    }

    fn runtime_requirements(&self) -> Cow<'_, RequirementsList> {
        (**self).runtime_requirements()
    }

    fn downstream_build_requirements<'a>(
        &self,
        components: impl IntoIterator<Item = &'a Component>,
    ) -> Cow<'_, RequirementsList> {
        (**self).downstream_build_requirements(components)
    }

    fn downstream_runtime_requirements<'a>(
        &self,
        components: impl IntoIterator<Item = &'a Component>,
    ) -> Cow<'_, RequirementsList> {
        (**self).downstream_build_requirements(components)
    }

    fn validation(&self) -> &super::ValidationSpec {
        (**self).validation()
    }

    fn build_script(&self) -> String {
        (**self).build_script()
    }

    fn validate_options(&self, given_options: &OptionMap) -> Compatibility {
        (**self).validate_options(given_options)
    }
}

impl<T: Package + Send + Sync> Package for &T {
    type Package = T::Package;

    fn ident(&self) -> &BuildIdent {
        (**self).ident()
    }

    fn option_values(&self) -> OptionMap {
        (**self).option_values()
    }

    fn sources(&self) -> &Vec<super::SourceSpec> {
        (**self).sources()
    }

    fn embedded(&self) -> &super::EmbeddedPackagesList {
        (**self).embedded()
    }

    fn embedded_as_packages(
        &self,
    ) -> std::result::Result<Vec<(Self::Package, Option<Component>)>, &str> {
        (**self).embedded_as_packages()
    }

    fn components(&self) -> &super::ComponentSpecList {
        (**self).components()
    }

    fn runtime_environment(&self) -> &Vec<super::EnvOp> {
        (**self).runtime_environment()
    }

    fn get_build_requirements(&self) -> crate::Result<Cow<'_, RequirementsList>> {
        (**self).get_build_requirements()
    }

    fn runtime_requirements(&self) -> Cow<'_, RequirementsList> {
        (**self).runtime_requirements()
    }

    fn downstream_build_requirements<'a>(
        &self,
        components: impl IntoIterator<Item = &'a Component>,
    ) -> Cow<'_, RequirementsList> {
        (**self).downstream_build_requirements(components)
    }

    fn downstream_runtime_requirements<'a>(
        &self,
        components: impl IntoIterator<Item = &'a Component>,
    ) -> Cow<'_, RequirementsList> {
        (**self).downstream_build_requirements(components)
    }

    fn validation(&self) -> &super::ValidationSpec {
        (**self).validation()
    }

    fn build_script(&self) -> String {
        (**self).build_script()
    }

    fn validate_options(&self, given_options: &OptionMap) -> Compatibility {
        (**self).validate_options(given_options)
    }
}
