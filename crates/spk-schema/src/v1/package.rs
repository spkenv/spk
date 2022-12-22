// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::borrow::Cow;

use itertools::Itertools;
use serde::{Deserialize, Serialize};
use spk_schema_foundation::version::CompatRule;
use spk_schema_foundation::version_range::Ranged;
use spk_schema_ident::{BuildIdent, PreReleasePolicy, RequestedBy};

use super::{PackageOption, SourceSpec, TestScript};
use crate::foundation::ident_build::Build;
use crate::foundation::ident_component::Component;
use crate::foundation::name::PkgName;
use crate::foundation::option_map::OptionMap;
use crate::foundation::spec_ops::prelude::*;
use crate::foundation::version::{Compat, Compatibility, Version};
use crate::ident::{is_false, PkgRequest, Satisfy, VarRequest};
use crate::meta::Meta;
use crate::{
    ComponentSpecList,
    Deprecate,
    DeprecateMut,
    EnvOp,
    PackageMut,
    RequirementsList,
    Result,
    ValidationSpec,
};

#[cfg(test)]
#[path = "./package_test.rs"]
mod package_test;

#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub environment: Vec<EnvOp>,
    #[serde(default)]
    pub components: ComponentSpecList<Self>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub test: Vec<TestScript>,
    #[serde(default, skip_serializing_if = "ValidationSpec::is_default")]
    pub validation: ValidationSpec,
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
            environment: Default::default(),
            components: Default::default(),
            test: Default::default(),
            validation: Default::default(),
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
            .map(|o| (o.var.name().clone(), o.var.value_or_default().to_owned()))
            .collect()
    }

    fn sources(&self) -> &Vec<crate::SourceSpec> {
        &self.source.collect
    }

    fn embedded<'a>(
        &self,
        components: impl IntoIterator<Item = &'a Component>,
    ) -> Vec<Self::EmbeddedStub> {
        self.components
            .resolve_uses(components)
            .flat_map(|c| &c.embedded)
            .cloned()
            .collect()
    }

    fn components(&self) -> Cow<'_, crate::ComponentSpecList<Self::EmbeddedStub>> {
        Cow::Borrowed(&self.components)
    }

    fn runtime_environment(&self) -> &Vec<EnvOp> {
        &self.environment
    }

    fn runtime_requirements<'a>(
        &self,
        components: impl IntoIterator<Item = &'a Component>,
    ) -> Cow<'_, RequirementsList> {
        let mut requirements = RequirementsList::new();
        let components = self.components.resolve_uses(components);
        for component in components {
            requirements.extend(component.requirements.iter().cloned())
        }
        Cow::Owned(requirements)
    }

    fn downstream_requirements<'a>(
        &self,
        components: impl IntoIterator<Item = &'a Component>,
    ) -> Cow<'_, RequirementsList> {
        let components: Vec<_> = components.into_iter().collect();
        Cow::Owned(
            self.options
                .iter()
                .filter(|o| {
                    o.propagation()
                        .at_downstream
                        .is_enabled_for(components.clone())
                })
                .filter_map(|o| {
                    o.to_request(None, || {
                        RequestedBy::UpstreamRequirement(self.pkg.to_owned())
                    })
                })
                .collect(),
        )
    }

    fn validation(&self) -> &ValidationSpec {
        &self.validation
    }

    fn build_script(&self) -> Cow<'_, String> {
        Cow::Borrowed(&self.script)
    }

    fn validate_options(&self, given_options: &OptionMap) -> Compatibility {
        let mut must_exist = given_options.package_options_without_global(self.name());
        let given_options = given_options.package_options(self.name());
        for option in self.options.iter() {
            let name = option.name();
            let value = given_options
                .get_for_package(self.pkg.name(), name)
                .map(String::as_str);
            let compat = option.validate(value);
            if !compat.is_ok() {
                return Compatibility::incompatible(format!("invalid value for {name}: {compat}",));
            }

            must_exist.remove(name.without_namespace());
        }

        if !must_exist.is_empty() {
            let missing = must_exist;
            return Compatibility::incompatible(format!(
                "Package does not define requested build options: {missing:?}",
            ));
        }

        Compatibility::Compatible
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
            return Compatibility::incompatible(format!(
                "different package name: {} != {}",
                pkg_request.pkg.name,
                self.pkg.name()
            ));
        }

        if self.is_deprecated() && pkg_request.pkg.build.as_ref() != Some(self.pkg.build()) {
            return Compatibility::incompatible(
                "Build is deprecated and was not specifically requested",
            );
        }

        if pkg_request.prerelease_policy == PreReleasePolicy::ExcludeAll
            && !self.version().pre.is_empty()
        {
            return Compatibility::incompatible("prereleases not allowed");
        }

        let source_package_requested = pkg_request.pkg.build == Some(Build::Source);
        let is_source_build = self.pkg.is_source() && !source_package_requested;
        if !pkg_request.pkg.components.is_empty() && !is_source_build {
            let required_components = self
                .components
                .resolve_uses_names(pkg_request.pkg.components.iter());
            let available_components = self.components.names_owned();
            let missing_components = required_components
                .difference(&available_components)
                .map(ToString::to_string)
                .collect_vec();
            if !missing_components.is_empty() {
                return Compatibility::incompatible(format!(
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

        Compatibility::incompatible(format!(
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
            .filter(|o| o.var.name() == &var_request.var);
        for option in options {
            let compat = option.check_satisfies_request(var_request);
            if !compat.is_ok() {
                return compat;
            }
        }
        Compatibility::Compatible
    }
}
