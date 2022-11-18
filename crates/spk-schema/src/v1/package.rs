// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::borrow::Cow;

use serde::{Deserialize, Serialize};
use spk_schema_ident::BuildIdent;

use crate::foundation::ident_build::Build;
use crate::foundation::ident_component::Component;
use crate::foundation::name::PkgName;
use crate::foundation::option_map::OptionMap;
use crate::foundation::spec_ops::prelude::*;
use crate::foundation::version::{Compat, Compatibility, Version};
use crate::ident::{is_false, PkgRequest, Satisfy, VarRequest};
use crate::meta::Meta;
use crate::{
    Deprecate,
    DeprecateMut,
    EnvOp,
    PackageMut,
    RequirementsList,
    Result,
    SourceSpec,
    ValidationSpec,
};

#[cfg(test)]
#[path = "./package_test.rs"]
mod package_test;

#[derive(Debug, Clone, Hash, PartialEq, Eq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct Package {
    pub pkg: BuildIdent,
    #[serde(default, skip_serializing_if = "Meta::is_default")]
    pub meta: Meta,
    #[serde(default, skip_serializing_if = "Compat::is_default")]
    pub compat: Compat,
    #[serde(default, skip_serializing_if = "is_false")]
    pub deprecated: bool,
}

impl Package {
    /// Create an empty spec for the identified package
    pub fn new(ident: BuildIdent) -> Self {
        Self {
            pkg: ident,
            meta: Meta::default(),
            compat: Compat::default(),
            deprecated: bool::default(),
        }
    }
}

impl Named for Package {
    fn name(&self) -> &PkgName {
        self.pkg.name()
    }
}

impl HasVersion for Package {
    fn version(&self) -> &Version {
        self.pkg.version()
    }
}

impl Versioned for Package {
    fn compat(&self) -> &Compat {
        &self.compat
    }
}

impl HasBuild for Package {
    fn build(&self) -> &Build {
        self.pkg.build()
    }
}

impl Deprecate for Package {
    fn is_deprecated(&self) -> bool {
        self.deprecated
    }
}

impl DeprecateMut for Package {
    fn deprecate(&mut self) -> Result<()> {
        self.deprecated = true;
        Ok(())
    }

    fn undeprecate(&mut self) -> Result<()> {
        self.deprecated = false;
        Ok(())
    }
}

impl crate::Package for Package {
    type EmbeddedStub = Self;

    fn ident(&self) -> &BuildIdent {
        &self.pkg
    }

    fn option_values(&self) -> OptionMap {
        todo!()
    }

    fn sources(&self) -> &Vec<SourceSpec> {
        todo!()
    }

    fn embedded<'a>(
        &self,
        _components: impl IntoIterator<Item = &'a Component>,
    ) -> Vec<Self::EmbeddedStub> {
        todo!()
    }

    fn components(&self) -> Cow<'_, crate::ComponentSpecList<Self::EmbeddedStub>> {
        todo!()
    }

    fn runtime_environment(&self) -> &Vec<EnvOp> {
        todo!()
    }

    fn runtime_requirements(&self) -> Cow<'_, RequirementsList> {
        todo!()
    }

    fn downstream_build_requirements<'a>(
        &self,
        _components: impl IntoIterator<Item = &'a Component>,
    ) -> Cow<'_, RequirementsList > {
        todo!()
    }

    fn downstream_runtime_requirements<'a>(
        &self,
        _components: impl IntoIterator<Item = &'a Component>,
    ) -> Cow<'_, RequirementsList > {
        todo!()
    }

    fn validation(&self) -> &ValidationSpec {
        todo!()
    }

    fn build_script(&self) -> String {
        todo!()
    }

    fn validate_options(&self, _given_options: &OptionMap) -> Compatibility {
        todo!()
    }
}

impl PackageMut for Package {
    fn set_build(&mut self, build: Build) {
        self.pkg.set_target(build);
    }
}

impl Satisfy<PkgRequest> for Package {
    fn check_satisfies_request(&self, _pkg_request: &PkgRequest) -> Compatibility {
        todo!()
    }
}

impl Satisfy<VarRequest> for Package {
    fn check_satisfies_request(&self, _var_request: &VarRequest) -> Compatibility {
        todo!()
    }
}
