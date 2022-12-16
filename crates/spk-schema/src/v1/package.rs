// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::borrow::Cow;

use itertools::Itertools;
use serde::{Deserialize, Serialize};
use spk_schema_foundation::version::CompatRule;
use spk_schema_foundation::version_range::Ranged;
use spk_schema_ident::{BuildIdent, PreReleasePolicy, RequestedBy};

use super::{PackageOption, PackagePackagingSpec, SourceSpec};
use crate::foundation::ident_build::Build;
use crate::foundation::ident_component::Component;
use crate::foundation::name::PkgName;
use crate::foundation::option_map::OptionMap;
use crate::foundation::spec_ops::prelude::*;
use crate::foundation::version::{Compat, Compatibility, Version};
use crate::ident::{is_false, PkgRequest, Satisfy, VarRequest};
use crate::meta::Meta;
use crate::{Deprecate, DeprecateMut, EnvOp, PackageMut, RequirementsList, Result, ValidationSpec};

#[cfg(test)]
#[path = "./package_test.rs"]
mod package_test;

#[derive(Debug, Clone, Hash, PartialEq, Eq, Ord, PartialOrd, Serialize, Deserialize)]
#[cfg_attr(test, serde(deny_unknown_fields))]
pub struct Package {
    pub pkg: BuildIdent,
    #[serde(default, skip_serializing_if = "Meta::is_default")]
    pub meta: Meta,
    #[serde(default, skip_serializing_if = "Compat::is_default")]
    pub compat: Compat,
    #[serde(default, skip_serializing_if = "is_false")]
    pub deprecated: bool,
    #[serde(default, skip_serializing_if = "SourceSpec::is_empty")]
    pub source: SourceSpec,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<PackageOption>,
    #[serde(default)]
    #[serde(default = "Package::default_script")]
    pub script: String,
}

impl Package {
    /// Create an empty spec for the identified package
    pub fn new(ident: BuildIdent) -> Self {
        Self {
            pkg: ident,
            meta: Meta::default(),
            compat: Compat::default(),
            deprecated: bool::default(),
            source: Default::default(),
            options: Default::default(),
            script: Self::default_script(),
        }
    }

    fn default_script() -> String {
        String::from("bash build.sh")
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
        self.options
            .iter()
            .filter_map(|o| match o {
                PackageOption::Var(v) => Some(v),
                _ => None,
            })
            .map(|o| (o.var.0.clone(), o.var.1.clone().unwrap_or_default()))
            .collect()
    }

    fn sources(&self) -> &Vec<crate::SourceSpec> {
        &self.source.collect
    }

    fn embedded<'a>(
        &self,
        components: impl IntoIterator<Item = &'a Component>,
    ) -> Vec<Self::EmbeddedStub> {
        self.package
            .components
            .resolve_uses(components)
            .flat_map(|c| &c.embedded)
            .cloned()
            .collect()
    }

    fn components(&self) -> Cow<'_, crate::ComponentSpecList<Self::EmbeddedStub>> {
        Cow::Borrowed(&self.package.components)
    }

    fn runtime_environment(&self) -> &Vec<EnvOp> {
        &self.package.environment
    }

    fn runtime_requirements<'a>(
        &self,
        components: impl IntoIterator<Item = &'a Component>,
    ) -> Cow<'_, RequirementsList> {
        let mut requirements = RequirementsList::new();
        let components = self.package.components.resolve_uses(components);
        for component in components {
            requirements.extend(component.requirements.iter().cloned())
        }
        Cow::Owned(requirements)
    }

    fn downstream_build_requirements<'a>(
        &self,
        _components: impl IntoIterator<Item = &'a Component>,
    ) -> Cow<'_, RequirementsList> {
        Cow::Owned(
            self.options
                .iter()
                .filter(|o| o.propagation().at_downstream_build)
                .filter_map(|o| {
                    o.to_request(|| RequestedBy::UpstreamBuildRequirement(self.pkg.to_owned()))
                })
                .collect(),
        )
    }

    fn downstream_runtime_requirements<'a>(
        &self,
        _components: impl IntoIterator<Item = &'a Component>,
    ) -> Cow<'_, RequirementsList> {
        Cow::Owned(
            self.options
                .iter()
                .filter(|o| o.propagation().at_downstream_runtime)
                .filter_map(|o| {
                    o.to_request(|| RequestedBy::UpstreamRuntimeRequirement(self.pkg.to_owned()))
                })
                .collect(),
        )
    }

    fn validation(&self) -> &ValidationSpec {
        &self.package.validation
    }

    fn build_script(&self) -> Cow<'_, String> {
        Cow::Borrowed(&self.script)
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
    fn check_satisfies_request(&self, pkg_request: &PkgRequest) -> Compatibility {
        if pkg_request.pkg.name != *self.pkg.name() {
            return Compatibility::Incompatible(format!(
                "different package name: {} != {}",
                pkg_request.pkg.name,
                self.pkg.name()
            ));
        }

        if self.is_deprecated() && pkg_request.pkg.build.as_ref() != Some(self.pkg.build()) {
            return Compatibility::Incompatible(
                "Build is deprecated and was not specifically requested".to_string(),
            );
        }

        if pkg_request.prerelease_policy == PreReleasePolicy::ExcludeAll
            && !self.version().pre.is_empty()
        {
            return Compatibility::Incompatible("prereleases not allowed".to_string());
        }

        let source_package_requested = pkg_request.pkg.build == Some(Build::Source);
        let is_source_build = self.pkg.is_source() && !source_package_requested;
        if !pkg_request.pkg.components.is_empty() && !is_source_build {
            let required_components = self
                .package
                .components
                .resolve_uses_names(pkg_request.pkg.components.iter());
            let available_components = self.package.components.names_owned();
            let missing_components = required_components
                .difference(&available_components)
                .map(ToString::to_string)
                .collect_vec();
            if !missing_components.is_empty() {
                return Compatibility::Incompatible(format!(
                    "does not define requested components: [{}], found [{}]",
                    missing_components.join(", "),
                    available_components
                        .iter()
                        .map(Component::to_string)
                        .sorted()
                        .join(", ")
                ));
            }
        }

        let c = pkg_request
            .pkg
            .version
            .is_satisfied_by(self, CompatRule::Binary);
        if !c.is_ok() {
            return c;
        }

        if pkg_request.pkg.build.is_none()
            || pkg_request.pkg.build.as_ref() == Some(self.pkg.build())
        {
            return Compatibility::Compatible;
        }

        Compatibility::Incompatible(format!(
            "Package and request differ in builds: requested {:?}, got {:?}",
            pkg_request.pkg.build,
            self.pkg.build()
        ))
    }
}

impl Satisfy<VarRequest> for Package {
    fn check_satisfies_request(&self, var_request: &VarRequest) -> Compatibility {
        let options = self
            .options
            .iter()
            .filter_map(PackageOption::as_var)
            .filter(|o| o.var.0 == var_request.var);
        for option in options {
            let compat = option.check_satisfies_request(var_request);
            if !compat.is_ok() {
                return compat;
            }
        }
        Compatibility::Compatible
    }
}
