// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use crate::Result;

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

    /// Return true if this package has been deprecated
    fn deprecated(&self) -> bool;

    /// The packages that are embedded within this one
    fn embedded(&self) -> &super::EmbeddedPackagesList;

    /// The components defined by this package
    fn components(&self) -> &super::ComponentSpecList;

    /// Requests that must be met to use this package
    fn runtime_requirements(&self) -> &super::RequirementsList;

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

    fn embedded(&self) -> &super::EmbeddedPackagesList {
        (**self).embedded()
    }

    fn components(&self) -> &super::ComponentSpecList {
        (**self).components()
    }

    fn runtime_requirements(&self) -> &super::RequirementsList {
        (**self).runtime_requirements()
    }

    fn update_for_build(
        &self,
        options: &super::OptionMap,
        build_env: &crate::solve::Solution,
    ) -> Result<super::Spec> {
        (**self).update_for_build(options, build_env)
    }
}
