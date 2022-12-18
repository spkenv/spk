// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::borrow::Cow;
use std::path::Path;

use serde::{Deserialize, Serialize};
use spk_schema_foundation::ident_build::Build;
use spk_schema_foundation::ident_component::Component;
use spk_schema_ident::{NameAndValue, VersionIdent};

use super::{RecipeBuildSpec, RecipeOptionList, RecipePackagingSpec, SourceSpec};
use crate::foundation::name::PkgName;
use crate::foundation::option_map::OptionMap;
use crate::foundation::spec_ops::prelude::*;
use crate::foundation::version::{Compat, Compatibility, Version};
use crate::ident::{is_false, PkgRequest, Satisfy, VarRequest};
use crate::meta::Meta;
use crate::{
    BuildEnv,
    BuildEnvMember,
    ComponentSpec,
    ComponentSpecList,
    Deprecate,
    DeprecateMut,
    Package,
    RequirementsList,
    Result,
    TestStage,
};

#[cfg(test)]
#[path = "./recipe_test.rs"]
mod recipe_test;

#[derive(Debug, Clone, Hash, PartialEq, Eq, Ord, PartialOrd, Serialize, Deserialize)]
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
    #[serde(default, skip_serializing_if = "SourceSpec::is_empty")]
    pub source: SourceSpec,
    #[serde(default)]
    pub build: RecipeBuildSpec,
    #[serde(default)]
    pub package: RecipePackagingSpec,
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
            source: Default::default(),
            build: Default::default(),
            package: Default::default(),
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
    type Test = super::TestScript;
    type Variant = super::VariantSpec;

    fn ident(&self) -> &VersionIdent {
        &self.pkg
    }

    fn default_variants(&self) -> Cow<'_, Vec<Self::Variant>> {
        if self.build.variants.is_empty() {
            Cow::Owned(vec![Default::default()])
        } else {
            Cow::Borrowed(&self.build.variants)
        }
    }

    fn resolve_options(&self, given: &OptionMap) -> Result<OptionMap> {
        self.options.resolve(given)
    }

    fn get_build_requirements(&self, options: &OptionMap) -> Result<Cow<'_, RequirementsList>> {
        Ok(Cow::Owned(
            self.options
                .iter()
                .filter(|o| o.check_is_active_at_build(options).is_ok())
                .filter_map(|o| o.to_request())
                .collect(),
        ))
    }

    fn get_tests(&self, stage: TestStage, _options: &OptionMap) -> Result<Vec<super::TestScript>> {
        match stage {
            TestStage::Sources => Ok(self.source.test.clone()),
            TestStage::Build => Ok(self.build.test.clone()),
            TestStage::Install => Ok(self.package.test.clone()),
        }
    }

    fn generate_source_build(&self, root: &Path) -> Result<Self::Output> {
        let mut source_build = super::Package::new(self.pkg.to_build(Build::Source));
        source_build.source = self.source.clone();
        for source in source_build.source.collect.iter_mut() {
            if let crate::SourceSpec::Local(source) = source {
                source.path = root.join(&source.path);
            }
        }
        source_build.components.clear();
        source_build
            .components
            .push(ComponentSpec::new(Component::Source));
        Ok(source_build)
    }

    fn generate_binary_build<E>(&self, build_env: E) -> Result<Self::Output>
    where
        E: BuildEnv,
        E::Package: Satisfy<PkgRequest>,
    {
        let build_options = build_env.options();
        let build_digest = self.resolve_options(&build_options)?.digest();
        let pkg = self.pkg.to_build(Build::Digest(build_digest));
        let options = self
            .options
            .iter()
            .filter(|option| option.check_is_active_at_build(&build_options).is_ok())
            .map(|option| {
                let propagation = super::package_option::Propagation {
                    at_runtime: option.check_is_active_at_runtime(&build_env).is_ok(),
                    at_downstream: option.check_is_active_at_downstream(&build_env).is_ok(),
                };
                match option {
                    super::RecipeOption::Pkg(opt) => {
                        super::PackageOption::Pkg(Box::new(super::package_option::PkgOption {
                            pkg: opt.pkg.clone(),
                            propagation,
                        }))
                    }
                    super::RecipeOption::Var(opt) => {
                        let name = || opt.var.name().clone();
                        let var = build_options
                            .get_for_package(self.pkg.name(), opt.var.name())
                            .or_else(|| opt.var.value(build_options.get(opt.var.name())))
                            .map(|v| NameAndValue::WithAssignedValue(name(), v.clone()))
                            .unwrap_or_else(|| NameAndValue::NameOnly(name()));
                        super::PackageOption::Var(Box::new(super::package_option::VarOption {
                            var,
                            choices: opt.choices.clone(),
                            propagation,
                        }))
                    }
                }
            })
            .collect();

        let components: ComponentSpecList<_> = self
            .package
            .components
            .iter()
            .filter(|c| c.when.check_is_active(&build_env).is_ok())
            .map(|c| (**c).clone())
            .collect();
        let test = self.package.test.clone();
        let script = self.build.script.to_string(&build_env);
        Ok(super::Package {
            pkg,
            meta: self.meta.clone(),
            deprecated: false,
            compat: self.compat.clone(),
            source: self.source.clone(),
            options,
                environment: self.package.environment.clone(),
                components,
                test,
                validation: self.package.validation.clone(),
            script,
        })
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
