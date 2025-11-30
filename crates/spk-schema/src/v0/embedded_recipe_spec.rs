// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use serde::{Deserialize, Serialize};
use spk_schema_foundation::IsDefault;
use spk_schema_foundation::ident::{AsVersionIdent, BuildIdent, VersionIdent};
use spk_schema_foundation::ident_build::{Build, EmbeddedSource};
use spk_schema_foundation::option_map::OptionMap;

use super::TestSpec;
use crate::foundation::name::PkgName;
use crate::foundation::spec_ops::prelude::*;
use crate::foundation::version::{Compat, Version};
use crate::ident::is_false;
use crate::metadata::Meta;
use crate::v0::{EmbeddedBuildSpec, EmbeddedPackageSpec, EmbeddedRecipeInstallSpec};
use crate::{
    ComponentSpecList,
    Components,
    Deprecate,
    DeprecateMut,
    EnvOp,
    RecipeComponentSpec,
    Result,
    RuntimeEnvironment,
    SourceSpec,
};

#[cfg(test)]
#[path = "./embedded_recipe_spec_test.rs"]
mod embedded_recipe_spec_test;

/// A recipe for an embedded package that can appear within parent package
/// recipe.
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
    pub install: EmbeddedRecipeInstallSpec,
}

impl EmbeddedRecipeSpec {
    pub fn render_all_pins(
        self,
        options: &OptionMap,
        resolved_by_name: &std::collections::HashMap<&PkgName, &BuildIdent>,
    ) -> Result<EmbeddedPackageSpec> {
        Ok(EmbeddedPackageSpec {
            pkg: self
                .pkg
                .into_build_ident(Build::Embedded(EmbeddedSource::Unknown)),
            meta: self.meta,
            compat: self.compat,
            deprecated: self.deprecated,
            sources: self.sources,
            build: self.build,
            tests: self.tests,
            install: self.install.render_all_pins(options, resolved_by_name)?,
        })
    }
}

impl AsVersionIdent for EmbeddedRecipeSpec {
    fn as_version_ident(&self) -> &VersionIdent {
        self.pkg.as_version_ident()
    }
}

impl Components for EmbeddedRecipeSpec {
    type ComponentSpecT = RecipeComponentSpec;

    fn components(&self) -> &ComponentSpecList<Self::ComponentSpecT> {
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
            install: pkg_spec.install.into(),
        }
    }
}
