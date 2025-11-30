// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::borrow::Cow;
use std::collections::HashMap;

use spk_schema_foundation::ident::{BuildIdent, PinnedRequest};
use spk_schema_foundation::ident_build::Build;
use spk_schema_foundation::option_map::OptFilter;
use spk_schema_foundation::spec_ops::{HasBuildIdent, Named, Versioned};
use spk_schema_foundation::version::VERSION_SEP;

use super::RequirementsList;
use crate::foundation::ident_component::Component;
use crate::foundation::option_map::OptionMap;
use crate::spec::SpecTest;
use crate::{ComponentSpec, DeprecateMut, Opt, RuntimeEnvironment};

#[cfg(test)]
#[path = "./package_test.rs"]
mod package_test;

/// Macro to forward trait implementations to references, boxes, and Arcs
macro_rules! forward_to_impl {
    ($trait_name:ident, { $($item:item)* }) => {
        impl<T: $trait_name + Send + Sync> $trait_name for std::sync::Arc<T> {
            $($item)*
        }

        impl<T: $trait_name + Send + Sync> $trait_name for Box<T> {
            $($item)*
        }

        impl<T: $trait_name + Send + Sync> $trait_name for &T {
            $($item)*
        }
    };
}

/// Access to the components defined by a package.
pub trait Components {
    type ComponentSpecT;

    /// The components defined by this package
    fn components(&self) -> &super::ComponentSpecList<Self::ComponentSpecT>;
}

forward_to_impl!(Components, {
    type ComponentSpecT = T::ComponentSpecT;

    fn components(&self) -> &super::ComponentSpecList<Self::ComponentSpecT> {
        (**self).components()
    }
});

/// Access to the option values defined by a package.
pub trait OptionValues {
    /// The values for this package's options used for this build.
    fn option_values(&self) -> OptionMap;
}

forward_to_impl!(OptionValues, {
    fn option_values(&self) -> OptionMap {
        (**self).option_values()
    }
});

/// Can be resolved into an environment.
#[enum_dispatch::enum_dispatch]
pub trait Package:
    Named
    + HasBuildIdent
    + Versioned
    + super::Deprecate
    + RuntimeEnvironment
    + Components<ComponentSpecT = ComponentSpec>
    + OptionValues
    + Clone
    + Eq
    + std::hash::Hash
    + Sync
    + Send
{
    type Package;
    type EmbeddedPackage;

    /// The full identifier for this package
    ///
    /// This includes the version and optional build
    fn ident(&self) -> &BuildIdent;

    /// The additional metadata attached to this package
    fn metadata(&self) -> &crate::metadata::Meta;

    /// Returns true if the spec's options match all the given option
    /// filters, otherwise false
    fn matches_all_filters(&self, filter_by: &Option<Vec<OptFilter>>) -> bool;

    /// Return the location of sources for this package
    fn sources(&self) -> &Vec<super::SourceSpec>;

    /// The packages that are embedded within this one
    fn embedded(&self) -> &super::EmbeddedPackagesList<Self::EmbeddedPackage>;

    /// The packages that are embedded within this one.
    ///
    /// Return both top-level embedded packages and packages that are
    /// embedded inside a component. The returned list is a pair of the
    /// embedded package and the component it came from, if any.
    #[allow(clippy::type_complexity)]
    fn embedded_as_packages(
        &self,
    ) -> std::result::Result<Vec<(Self::Package, Option<Component>)>, &str>;

    /// The list of build options for this package
    fn get_build_options(&self) -> &Vec<Opt>;

    /// Identify the requirements for a build of this package.
    fn get_build_requirements(&self) -> crate::Result<Cow<'_, RequirementsList<PinnedRequest>>>;

    /// Return the environment variables to be set for a build of the given package spec.
    fn get_build_env(&self) -> HashMap<String, String> {
        let mut env = HashMap::with_capacity(8);
        env.insert("SPK_PKG".to_string(), self.ident().to_string());
        env.insert("SPK_PKG_NAME".to_string(), self.name().to_string());
        env.insert("SPK_PKG_VERSION".to_string(), self.version().to_string());
        env.insert(
            "SPK_PKG_BUILD".to_string(),
            self.ident().build().to_string(),
        );
        env.insert(
            "SPK_PKG_VERSION_MAJOR".to_string(),
            self.version().major().to_string(),
        );
        env.insert(
            "SPK_PKG_VERSION_MINOR".to_string(),
            self.version().minor().to_string(),
        );
        env.insert(
            "SPK_PKG_VERSION_PATCH".to_string(),
            self.version().patch().to_string(),
        );
        env.insert(
            "SPK_PKG_VERSION_BASE".to_string(),
            self.version()
                .parts
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(VERSION_SEP),
        );
        env
    }

    /// Requests that must be met to use this package
    fn runtime_requirements(&self) -> Cow<'_, RequirementsList<PinnedRequest>>;

    /// Package's test specs for all test stages
    fn get_all_tests(&self) -> Vec<SpecTest>;

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
}

pub trait PackageMut: Package + DeprecateMut {
    /// Modify the build identifier for this package
    fn set_build(&mut self, build: Build);
}

forward_to_impl!(Package, {
    type Package = T::Package;
    type EmbeddedPackage = T::EmbeddedPackage;

    fn ident(&self) -> &BuildIdent {
        (**self).ident()
    }

    fn metadata(&self) -> &crate::metadata::Meta {
        (**self).metadata()
    }

    fn matches_all_filters(&self, filter_by: &Option<Vec<OptFilter>>) -> bool {
        (**self).matches_all_filters(filter_by)
    }

    fn sources(&self) -> &Vec<super::SourceSpec> {
        (**self).sources()
    }

    fn embedded(&self) -> &super::EmbeddedPackagesList<Self::EmbeddedPackage> {
        (**self).embedded()
    }

    fn embedded_as_packages(
        &self,
    ) -> std::result::Result<Vec<(Self::Package, Option<Component>)>, &str> {
        (**self).embedded_as_packages()
    }

    fn get_build_options(&self) -> &Vec<Opt> {
        (**self).get_build_options()
    }

    fn get_build_requirements(&self) -> crate::Result<Cow<'_, RequirementsList<PinnedRequest>>> {
        (**self).get_build_requirements()
    }

    fn runtime_requirements(&self) -> Cow<'_, RequirementsList<PinnedRequest>> {
        (**self).runtime_requirements()
    }

    fn get_all_tests(&self) -> Vec<SpecTest> {
        (**self).get_all_tests()
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
        (**self).downstream_runtime_requirements(components)
    }

    fn validation(&self) -> &super::ValidationSpec {
        (**self).validation()
    }

    fn build_script(&self) -> String {
        (**self).build_script()
    }
});
