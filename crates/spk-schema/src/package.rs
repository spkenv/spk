// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use spk_schema_foundation::ident_build::Build;
use spk_schema_foundation::spec_ops::{Named, Versioned};

use crate::foundation::ident_component::Component;
use crate::foundation::option_map::OptionMap;
use crate::foundation::version::Compatibility;
use crate::ident::Ident;
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

    // fn components_iter(&self) -> std::slice::Iter<'_, Self::Component>;

    /// The full identifier for this package
    ///
    /// This includes the version and optional build
    fn ident(&self) -> &Ident;

    // fn is_satisfied_by_var_request(&self, var_request: &Self::VarRequest) -> Compatibility;

    /// The values for this packages options used for this build.
    fn option_values(&self) -> OptionMap;

    /// The input options for this package
    fn options(&self) -> &Vec<super::Opt>;

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

    /// Requests that must be met to use this package
    fn runtime_requirements(&self) -> &super::RequirementsList;

    /// Return the set of configured validators when building this package
    fn validation(&self) -> &super::ValidationSpec;

    /// Return the build script for building package
    fn build_script(&self) -> String;

    /// Validate the given options against the options in this spec.
    fn validate_options(&self, given_options: &OptionMap) -> Compatibility {
        let mut must_exist = given_options.package_options_without_global(self.name());
        let given_options = given_options.package_options(self.name());
        for option in self.options().iter() {
            let value = given_options
                .get(option.full_name().without_namespace())
                .map(String::as_str);
            let compat = option.validate(value);
            if !compat.is_ok() {
                return Compatibility::Incompatible(format!(
                    "invalid value for {}: {compat}",
                    option.full_name(),
                ));
            }

            must_exist.remove(option.full_name().without_namespace());
        }

        if !must_exist.is_empty() {
            let missing = must_exist;
            return Compatibility::Incompatible(format!(
                "Package does not define requested build options: {missing:?}",
            ));
        }

        Compatibility::Compatible
    }
}

pub trait PackageMut: Package + DeprecateMut {
    /// Modify the build identifier for this package
    fn set_build(&mut self, build: Build);
}

impl<T: Package + Send + Sync> Package for std::sync::Arc<T> {
    type Package = T::Package;

    fn ident(&self) -> &Ident {
        (**self).ident()
    }

    fn option_values(&self) -> OptionMap {
        (**self).option_values()
    }

    fn options(&self) -> &Vec<super::Opt> {
        (**self).options()
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

    fn runtime_requirements(&self) -> &super::RequirementsList {
        (**self).runtime_requirements()
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

    fn ident(&self) -> &Ident {
        (**self).ident()
    }

    fn option_values(&self) -> OptionMap {
        (**self).option_values()
    }

    fn options(&self) -> &Vec<super::Opt> {
        (**self).options()
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

    fn runtime_requirements(&self) -> &super::RequirementsList {
        (**self).runtime_requirements()
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
