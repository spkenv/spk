// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::borrow::Cow;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use spk_schema_foundation::IsDefault;
use spk_schema_foundation::ident::{AsVersionIdent, BuildIdent, VersionIdent};
use spk_schema_foundation::ident_build::EmbeddedSource;
use spk_schema_foundation::version::{IncompatibleReason, VarOptionProblem};

use super::TestSpec;
use crate::foundation::ident_build::Build;
use crate::foundation::name::PkgName;
use crate::foundation::spec_ops::prelude::*;
use crate::foundation::version::{Compat, Compatibility, Version};
use crate::ident::{PkgRequest, Satisfy, VarRequest, is_false};
use crate::metadata::Meta;
use crate::v0::{
    EmbeddedBuildSpec,
    EmbeddedInstallSpec,
    EmbeddedRecipeSpec,
    check_package_spec_satisfies_pkg_request,
};
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
#[path = "./embedded_package_spec_test.rs"]
mod embedded_package_spec_test;

/// A built package specification for an embedded package.
///
/// This is similar to [`super::PackageSpec`], but is used for the packages that
/// are embedded within a parent package.
#[derive(Debug, Deserialize, Clone, Hash, PartialEq, Eq, Ord, PartialOrd, Serialize)]
pub struct EmbeddedPackageSpec {
    pub pkg: BuildIdent,
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

impl EmbeddedPackageSpec {
    /// Create an empty spec for the identified package
    pub fn new(ident: BuildIdent) -> Self {
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

impl EmbeddedPackageSpec {
    pub fn ident(&self) -> &BuildIdent {
        &self.pkg
    }
}

impl AsVersionIdent for EmbeddedPackageSpec {
    fn as_version_ident(&self) -> &VersionIdent {
        self.pkg.as_version_ident()
    }
}

impl Components for EmbeddedPackageSpec {
    fn components(&self) -> &ComponentSpecList {
        &self.install.components
    }
}

impl Deprecate for EmbeddedPackageSpec {
    fn is_deprecated(&self) -> bool {
        self.deprecated
    }
}

impl DeprecateMut for EmbeddedPackageSpec {
    fn deprecate(&mut self) -> Result<()> {
        self.deprecated = true;
        Ok(())
    }

    fn undeprecate(&mut self) -> Result<()> {
        self.deprecated = false;
        Ok(())
    }
}

impl HasBuild for EmbeddedPackageSpec {
    fn build(&self) -> &Build {
        self.pkg.build()
    }
}

impl HasVersion for EmbeddedPackageSpec {
    fn version(&self) -> &Version {
        self.pkg.version()
    }
}

impl Named for EmbeddedPackageSpec {
    fn name(&self) -> &PkgName {
        self.pkg.name()
    }
}

impl RuntimeEnvironment for EmbeddedPackageSpec {
    fn runtime_environment(&self) -> &[EnvOp] {
        &self.install.environment
    }
}

impl Versioned for EmbeddedPackageSpec {
    fn compat(&self) -> &Compat {
        &self.compat
    }
}

impl Satisfy<PkgRequest> for EmbeddedPackageSpec {
    fn check_satisfies_request(&self, pkg_request: &PkgRequest) -> Compatibility {
        check_package_spec_satisfies_pkg_request(self, pkg_request)
    }
}

impl Satisfy<VarRequest> for EmbeddedPackageSpec
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

impl From<EmbeddedRecipeSpec> for EmbeddedPackageSpec {
    fn from(recipe: EmbeddedRecipeSpec) -> Self {
        Self {
            pkg: recipe
                .pkg
                .into_build_ident(Build::Embedded(EmbeddedSource::Unknown)),
            meta: recipe.meta,
            compat: recipe.compat,
            deprecated: recipe.deprecated,
            sources: recipe.sources,
            build: recipe.build,
            tests: recipe.tests,
            install: recipe.install,
        }
    }
}
