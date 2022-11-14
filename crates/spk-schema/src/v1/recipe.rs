// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::path::Path;

use serde::{Deserialize, Serialize};
use spk_schema_ident::VersionIdent;

use super::RecipeOptionList;
use crate::foundation::name::PkgName;
use crate::foundation::option_map::OptionMap;
use crate::foundation::spec_ops::prelude::*;
use crate::foundation::version::{Compat, Compatibility, Version};
use crate::ident::{is_false, PkgRequest, Request, Satisfy, VarRequest};
use crate::meta::Meta;
use crate::test_spec::TestSpec;
use crate::{BuildEnv, Deprecate, DeprecateMut, Package, Result};

#[cfg(test)]
#[path = "./recipe_test.rs"]
mod recipe_test;

#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, serde(deny_unknown_fields))]
pub struct Recipe {
    pub pkg: VersionIdent,
    #[serde(default, skip_serializing_if = "Meta::is_default")]
    pub meta: Meta,
    #[serde(default, skip_serializing_if = "Compat::is_default")]
    pub compat: Compat,
    #[serde(default, skip_serializing_if = "is_false")]
    pub deprecated: bool,
    #[serde(default, skip_serializing_if = "RecipeOptionList::is_empty")]
    pub options: RecipeOptionList,
}

impl Recipe {
    /// Create an empty spec for the identified package
    pub fn new(ident: VersionIdent) -> Self {
        Self {
            pkg: ident,
            meta: Meta::default(),
            compat: Compat::default(),
            deprecated: bool::default(),
            options: Default::default(),
        }
    }
}

impl Named for Recipe {
    fn name(&self) -> &PkgName {
        self.pkg.name()
    }
}

impl HasVersion for Recipe {
    fn version(&self) -> &Version {
        self.pkg.version()
    }
}

impl Versioned for Recipe {
    fn compat(&self) -> &Compat {
        &self.compat
    }
}

impl Deprecate for Recipe {
    fn is_deprecated(&self) -> bool {
        self.deprecated
    }
}

impl DeprecateMut for Recipe {
    fn deprecate(&mut self) -> Result<()> {
        self.deprecated = true;
        Ok(())
    }

    fn undeprecate(&mut self) -> Result<()> {
        self.deprecated = false;
        Ok(())
    }
}

impl crate::Recipe for Recipe {
    type Output = super::Package;

    fn ident(&self) -> &VersionIdent {
        &self.pkg
    }

    fn default_variants(&self) -> &[OptionMap] {
        todo!()
    }

    fn resolve_options(&self, _given: &OptionMap) -> Result<OptionMap> {
        todo!()
    }

    fn get_build_requirements(&self, _options: &OptionMap) -> Result<Vec<Request>> {
        todo!()
    }

    fn get_tests(&self, _options: &OptionMap) -> Result<Vec<TestSpec>> {
        todo!()
    }

    fn generate_source_build(&self, _root: &Path) -> Result<Self::Output> {
        todo!()
    }

    fn generate_binary_build<E, P>(
        &self,
        _options: &OptionMap,
        _build_env: &E,
    ) -> Result<Self::Output>
    where
        E: BuildEnv<Package = P>,
        P: Package,
    {
        todo!()
    }
}

impl Satisfy<PkgRequest> for Recipe {
    fn check_satisfies_request(&self, _pkg_request: &PkgRequest) -> Compatibility {
        todo!()
    }
}

impl Satisfy<VarRequest> for Recipe {
    fn check_satisfies_request(&self, _var_request: &VarRequest) -> Compatibility {
        todo!()
    }
}
