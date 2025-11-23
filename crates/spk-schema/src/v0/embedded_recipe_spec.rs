// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::borrow::Cow;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use spk_schema_foundation::IsDefault;
use spk_schema_foundation::ident::{AsVersionIdent, VersionIdent};
use spk_schema_foundation::version::{IncompatibleReason, VarOptionProblem};

use super::TestSpec;
use crate::foundation::name::PkgName;
use crate::foundation::spec_ops::prelude::*;
use crate::foundation::version::{Compat, Compatibility, Version};
use crate::ident::{Satisfy, VarRequest, is_false};
use crate::metadata::Meta;
use crate::v0::{EmbeddedBuildSpec, EmbeddedInstallSpec, EmbeddedPackageSpec};
use crate::{
    ComponentSpecList,
    Components,
    Deprecate,
    DeprecateMut,
    EnvOp,
    Opt,
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

impl EmbeddedRecipeSpec {
    /// Create an empty spec for the identified package
    pub fn new(ident: VersionIdent) -> Self {
        Self {
            pkg: ident,
            meta: Meta::default(),
            compat: Compat::default(),
            deprecated: bool::default(),
            sources: Vec::new(),
            build: EmbeddedBuildSpec::default(),
            tests: Vec::new(),
            install: EmbeddedInstallSpec::default(),
        }
    }

    pub fn build_options(&self) -> Cow<'_, [Opt]> {
        Cow::Borrowed(self.build.options.as_slice())
    }
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

impl Satisfy<VarRequest> for EmbeddedRecipeSpec
where
    Self: Named,
{
    fn check_satisfies_request(&self, var_request: &VarRequest) -> Compatibility {
        let opt_required = var_request.var.namespace() == Some(self.name());
        let mut opt: Option<&Opt> = None;
        let request_name = &var_request.var;
        for o in self.build.options.iter() {
            if request_name == o.full_name() {
                opt = Some(o);
                break;
            }
            if request_name == &o.full_name().with_namespace(self.name()) {
                opt = Some(o);
                break;
            }
        }

        match opt {
            None => {
                if opt_required {
                    return Compatibility::Incompatible(IncompatibleReason::VarOptionMissing(
                        var_request.var.clone(),
                    ));
                }
                Compatibility::Compatible
            }
            Some(Opt::Pkg(opt)) => opt.validate(var_request.value.as_pinned()),
            Some(Opt::Var(opt)) => {
                let request_value = var_request.value.as_pinned();
                let exact = opt.get_value(request_value);
                if exact.as_deref() == request_value {
                    return Compatibility::Compatible;
                }

                // For values that aren't exact matches, if the option specifies
                // a compat rule, try treating the values as version numbers
                // and see if they satisfy the rule.
                if let Some(compat) = &opt.compat {
                    let base_version = exact.clone();
                    let Ok(base_version) = Version::from_str(&base_version.unwrap_or_default())
                    else {
                        return Compatibility::Incompatible(IncompatibleReason::VarOptionMismatch(
                            VarOptionProblem::IncompatibleBuildOptionInvalidVersion {
                                var_request: var_request.var.clone(),
                                base: exact.unwrap_or_default(),
                                request_value: request_value.unwrap_or_default().to_string(),
                            },
                        ));
                    };

                    let Ok(request_version) = Version::from_str(request_value.unwrap_or_default())
                    else {
                        return Compatibility::Incompatible(IncompatibleReason::VarOptionMismatch(
                            VarOptionProblem::IncompatibleBuildOptionInvalidVersion {
                                var_request: var_request.var.clone(),
                                base: exact.unwrap_or_default(),
                                request_value: request_value.unwrap_or_default().to_string(),
                            },
                        ));
                    };

                    let result = compat.is_binary_compatible(&base_version, &request_version);
                    if let Compatibility::Incompatible(incompatible) = result {
                        return Compatibility::Incompatible(IncompatibleReason::VarOptionMismatch(
                            VarOptionProblem::IncompatibleBuildOptionWithContext {
                                var_request: var_request.var.clone(),
                                exact: exact.unwrap_or_else(|| "None".to_string()),
                                request_value: request_value.unwrap_or_default().to_string(),
                                context: Box::new(incompatible),
                            },
                        ));
                    }
                    return result;
                }

                Compatibility::Incompatible(IncompatibleReason::VarOptionMismatch(
                    VarOptionProblem::IncompatibleBuildOption {
                        var_request: var_request.var.clone(),
                        exact: exact.unwrap_or_else(|| "None".to_string()),
                        request_value: request_value.unwrap_or_default().to_string(),
                    },
                ))
            }
        }
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
