// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use serde::{Deserialize, Serialize};
use spk_schema_foundation::IsDefault;
use spk_schema_foundation::ident::{AsVersionIdent, VersionIdent};

use super::TestSpec;
use crate::foundation::name::PkgName;
use crate::foundation::spec_ops::prelude::*;
use crate::foundation::version::{Compat, Version};
use crate::ident::is_false;
use crate::metadata::Meta;
use crate::v0::{EmbeddedBuildSpec, EmbeddedInstallSpec, EmbeddedPackageSpec};
use crate::{
    ComponentSpecList,
    Components,
    Deprecate,
    DeprecateMut,
    EnvOp,
    Result,
    RuntimeEnvironment,
    SourceSpec,
};

#[cfg(test)]
#[path = "./embedded_recipe_spec_test.rs"]
mod embedded_recipe_spec_test;

/// A recipe specification for an embedded package.
///
/// This is similar to [`super::RecipeSpec`], but is used for the recipes of
/// packages that are embedded within a parent package. An embedded recipe may
/// not define variants or have embedded packages of its own.
#[derive(Debug, Deserialize, Clone, Hash, PartialEq, Eq, Ord, PartialOrd, Serialize)]
pub struct EmbeddedRecipeSpec {
    pub pkg: VersionIdent,
    #[serde(default, skip_serializing_if = "Meta::is_default")]
    pub meta: Meta,
    #[serde(default, skip_serializing_if = "Compat::is_default")]
    pub compat: Compat,
    #[serde(default, skip_serializing_if = "is_false")]
    pub deprecated: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<SourceSpec>,
    #[serde(default, skip_serializing_if = "EmbeddedBuildSpec::is_default")]
    pub build: EmbeddedBuildSpec,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tests: Vec<TestSpec>,
    #[serde(default, skip_serializing_if = "IsDefault::is_default")]
    pub install: EmbeddedInstallSpec,
}

impl AsVersionIdent for EmbeddedRecipeSpec {
    fn as_version_ident(&self) -> &VersionIdent {
        self.pkg.as_version_ident()
    }
}

impl Components for EmbeddedRecipeSpec {
    fn components(&self) -> &ComponentSpecList {
        &self.install.components
    }
}

impl Deprecate for EmbeddedRecipeSpec {
    fn is_deprecated(&self) -> bool {
        self.deprecated
    }
}

impl DeprecateMut for EmbeddedRecipeSpec {
    fn deprecate(&mut self) -> Result<()> {
        self.deprecated = true;
        Ok(())
    }

    fn undeprecate(&mut self) -> Result<()> {
        self.deprecated = false;
        Ok(())
    }
}

impl HasVersion for EmbeddedRecipeSpec {
    fn version(&self) -> &Version {
        self.pkg.version()
    }
}

impl Named for EmbeddedRecipeSpec {
    fn name(&self) -> &PkgName {
        self.pkg.name()
    }
}

impl RuntimeEnvironment for EmbeddedRecipeSpec {
    fn runtime_environment(&self) -> &[EnvOp] {
        &self.install.environment
    }
}

impl Versioned for EmbeddedRecipeSpec {
    fn compat(&self) -> &Compat {
        &self.compat
    }
}

impl From<EmbeddedPackageSpec> for EmbeddedRecipeSpec {
    fn from(pkg_spec: EmbeddedPackageSpec) -> Self {
        Self {
            pkg: pkg_spec.pkg.as_version_ident().clone(),
            meta: pkg_spec.meta,
            compat: pkg_spec.compat,
            deprecated: pkg_spec.deprecated,
            sources: pkg_spec.sources,
            build: pkg_spec.build,
            tests: pkg_spec.tests,
            install: pkg_spec.install,
        }
    }
}
