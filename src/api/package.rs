// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use crate::Result;

#[cfg(test)]
#[path = "./package_test.rs"]
mod package_test;

pub trait Recipe {}

#[enum_dispatch::enum_dispatch]
pub trait Package: Send {
    /// The name of this package
    fn name(&self) -> &super::PkgName {
        &self.ident().name
    }

    /// The version number of this package
    fn version(&self) -> &super::Version {
        &self.ident().version
    }

    /// The full identifier for this package
    ///
    /// This includes the version and optional build
    fn ident(&self) -> &super::Ident;

    /// The compatibility guaranteed by this package's version
    fn compat(&self) -> &super::Compat;

    /// The input options for this package
    fn options(&self) -> &Vec<super::Opt>;

    /// Return the default variants to be built for this package
    fn variants(&self) -> &Vec<super::OptionMap>;

    /// Return the location of sources for this package
    fn sources(&self) -> &Vec<super::SourceSpec>;

    /// Return the tests defined for this package
    fn tests(&self) -> &Vec<super::TestSpec>;

    /// Return true if this package has been deprecated
    fn deprecated(&self) -> bool;

    /// The packages that are embedded within this one
    fn embedded(&self) -> &super::EmbeddedPackagesList;

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

    /// Return the full set of resolved build options using the given ones.
    fn resolve_all_options(&self, given: &super::OptionMap) -> super::OptionMap {
        let mut resolved = super::OptionMap::default();
        for opt in self.options().iter() {
            let given_value = match opt.full_name().namespace() {
                Some(_) => given
                    .get(opt.full_name())
                    .or_else(|| given.get(opt.full_name().without_namespace())),
                None => given
                    .get(&opt.full_name().with_namespace(self.name()))
                    .or_else(|| given.get(opt.full_name())),
            };
            let value = opt.get_value(given_value.map(String::as_ref));
            resolved.insert(opt.full_name().to_owned(), value);
        }

        resolved
    }

    /// Validate the given options against the options in this spec.
    fn validate_options(&self, given_options: &super::OptionMap) -> super::Compatibility {
        let mut must_exist = given_options.package_options_without_global(self.name());
        let given_options = given_options.package_options(self.name());
        for option in self.options().iter() {
            let value = given_options
                .get(option.full_name().without_namespace())
                .map(String::as_str);
            let compat = option.validate(value);
            if !compat.is_ok() {
                return super::Compatibility::Incompatible(format!(
                    "invalid value for {}: {}",
                    option.full_name(),
                    compat
                ));
            }

            must_exist.remove(option.full_name().without_namespace());
        }

        if !must_exist.is_empty() {
            let missing = must_exist;
            return super::Compatibility::Incompatible(format!(
                "Package does not define requested build options: {missing:?}",
            ));
        }

        super::Compatibility::Compatible
    }

    /// Update this spec to represent a specific binary package build.
    /// TODO: update to return a BuildSpec type
    fn update_for_build(
        &self,
        options: &super::OptionMap,
        build_env: &crate::solve::Solution,
    ) -> Result<super::Spec>;
}

impl<T: Package + Send + Sync> Package for std::sync::Arc<T> {
    fn ident(&self) -> &super::Ident {
        (**self).ident()
    }

    fn compat(&self) -> &super::Compat {
        (**self).compat()
    }

    fn deprecated(&self) -> bool {
        (**self).deprecated()
    }

    fn options(&self) -> &Vec<super::Opt> {
        (**self).options()
    }

    fn variants(&self) -> &Vec<super::OptionMap> {
        (**self).variants()
    }

    fn sources(&self) -> &Vec<super::SourceSpec> {
        (**self).sources()
    }

    fn tests(&self) -> &Vec<super::TestSpec> {
        (**self).tests()
    }

    fn embedded(&self) -> &super::EmbeddedPackagesList {
        (**self).embedded()
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

    fn resolve_all_options(&self, given: &super::OptionMap) -> super::OptionMap {
        (**self).resolve_all_options(given)
    }

    fn build_script(&self) -> String {
        (**self).build_script()
    }

    fn validate_options(&self, given_options: &super::OptionMap) -> super::Compatibility {
        (**self).validate_options(given_options)
    }

    fn update_for_build(
        &self,
        options: &super::OptionMap,
        build_env: &crate::solve::Solution,
    ) -> Result<super::Spec> {
        (**self).update_for_build(options, build_env)
    }
}
