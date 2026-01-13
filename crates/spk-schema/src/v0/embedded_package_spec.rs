// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::borrow::Cow;

use serde::{Deserialize, Serialize};
use spk_schema_foundation::IsDefault;
use spk_schema_foundation::ident::{
    AsVersionIdent,
    BuildIdent,
    PkgRequestWithOptions,
    RequestWithOptions,
    Satisfy,
    VarRequest,
    VersionIdent,
    is_false,
};
use spk_schema_foundation::ident_build::EmbeddedSource;
use spk_schema_foundation::ident_component::Component;
use spk_schema_foundation::name::{OptName, OptNameBuf};
use spk_schema_foundation::option_map::OptionMap;
use spk_schema_foundation::spec_ops::HasBuildIdent;
use spk_schema_foundation::version::{Compat, Compatibility, Version};

use super::TestSpec;
use crate::foundation::ident_build::Build;
use crate::foundation::name::PkgName;
use crate::foundation::spec_ops::prelude::*;
use crate::metadata::Meta;
use crate::option::VarOpt;
use crate::package::{BuildOptions, OptionValues};
use crate::requirements_list::AsOptNameAndValue;
use crate::v0::{
    EmbeddedBuildSpec,
    EmbeddedInstallSpec,
    EmbeddedRecipeSpec,
    check_package_spec_satisfies_pkg_request,
};
use crate::{
    ComponentSpec,
    ComponentSpecList,
    Components,
    Deprecate,
    DeprecateMut,
    DownstreamRequirements,
    Inheritance,
    Opt,
    RequirementsList,
    Result,
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
    // This field is private to update `install_requirements_with_options`
    // when it is modified.
    #[serde(default, skip_serializing_if = "EmbeddedBuildSpec::is_default")]
    build: EmbeddedBuildSpec,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tests: Vec<TestSpec>,
    // This field is private to update `install_requirements_with_options`
    // when it is modified.
    #[serde(default, skip_serializing_if = "IsDefault::is_default")]
    install: EmbeddedInstallSpec,
    /// Install requirements with options included.
    ///
    /// This value is not serialized; it is populated when loading or when build
    /// or install are modified.
    #[serde(skip)]
    install_requirements_with_options: RequirementsList<RequestWithOptions>,
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
            install_requirements_with_options: RequirementsList::default(),
        }
    }

    /// Read-only access to the build spec
    #[inline]
    pub fn build(&self) -> &EmbeddedBuildSpec {
        &self.build
    }

    /// Read-write access to the build spec
    pub fn build_mut<F, R>(&mut self, f: F) -> R
    where
        F: FnOnce(&mut EmbeddedBuildSpec) -> R,
    {
        let r = f(&mut self.build);
        self.install_requirements_with_options =
            Self::calculate_install_requirements_with_options(&self.build, &self.install);
        r
    }

    fn calculate_install_requirements_with_options(
        build: &EmbeddedBuildSpec,
        install: &EmbeddedInstallSpec,
    ) -> RequirementsList<RequestWithOptions> {
        (build.options.iter(), &install.requirements).into()
    }

    /// Return downstream var requirements that match the given filter.
    fn downstream_requirements<F>(&self, filter: F) -> Cow<'_, RequirementsList<RequestWithOptions>>
    where
        F: FnMut(&VarOpt) -> bool,
    {
        let requests = self
            .build
            .options
            .iter()
            .filter_map(|opt| match opt {
                Opt::Var(v) => Some(v.with_default_namespace(self.name())),
                Opt::Pkg(_) => None,
            })
            .filter(filter)
            .map(|o| {
                VarRequest {
                    // we are assuming that the var here will have a value because
                    // this is a built binary package
                    value: o.get_value(None).unwrap_or_default().into(),
                    var: o.var,
                    description: o.description.clone(),
                }
            })
            .map(RequestWithOptions::Var);
        RequirementsList::<RequestWithOptions>::try_from_iter(requests)
            .map(Cow::Owned)
            .expect("build opts do not contain duplicates")
    }

    /// Read-only access to the install spec
    #[inline]
    pub fn install(&self) -> &EmbeddedInstallSpec {
        &self.install
    }

    /// Read-only access to install requirements with options
    #[inline]
    pub fn install_requirements_with_options(&self) -> &RequirementsList<RequestWithOptions> {
        &self.install_requirements_with_options
    }

    /// Create a binary package from the given recipe.
    pub fn new_binary_package_from_recipe<K, R>(
        recipe: EmbeddedRecipeSpec,
        options: &OptionMap,
        resolved_by_name: &std::collections::HashMap<K, R>,
    ) -> Result<Self>
    where
        K: Eq + std::hash::Hash,
        K: std::borrow::Borrow<PkgName>,
        R: HasBuildIdent + Versioned,
    {
        let install = recipe.install.render_all_pins(options, resolved_by_name)?;

        enum OptOrPair<'a> {
            Opt(&'a Opt),
            Pair((&'a OptNameBuf, &'a String)),
        }

        impl<'a> AsOptNameAndValue for OptOrPair<'a> {
            fn as_opt_name_and_value(&self) -> Option<(&OptName, String)> {
                match self {
                    OptOrPair::Opt(opt) => (*opt).as_opt_name_and_value(),
                    OptOrPair::Pair((name, value)) => Some((name, (*value).clone())),
                }
            }
        }

        Ok(Self {
            pkg: recipe
                .pkg
                .into_build_ident(Build::Embedded(EmbeddedSource::Unknown)),
            meta: recipe.meta,
            compat: recipe.compat,
            deprecated: recipe.deprecated,
            sources: recipe.sources,
            tests: recipe.tests,
            install_requirements_with_options: (
                // Override the recipe options with the context options
                recipe
                    .build
                    .options
                    .iter()
                    .map(OptOrPair::Opt)
                    .chain(options.iter().map(OptOrPair::Pair)),
                &install.requirements,
            )
                .into(),
            build: recipe.build.render_all_pins(options, resolved_by_name)?,
            install,
        })
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

impl BuildOptions for EmbeddedPackageSpec {
    fn build_options(&self) -> Cow<'_, [Opt]> {
        Cow::Borrowed(&self.build.options)
    }
}

impl Components for EmbeddedPackageSpec {
    type ComponentSpecT = ComponentSpec;

    fn components(&self) -> &ComponentSpecList<Self::ComponentSpecT> {
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

impl DownstreamRequirements for EmbeddedPackageSpec {
    fn downstream_build_requirements<'a>(
        &self,
        _components: impl IntoIterator<Item = &'a Component>,
    ) -> Cow<'_, RequirementsList<RequestWithOptions>> {
        self.downstream_requirements(|o| o.inheritance() != Inheritance::Weak)
    }

    fn downstream_runtime_requirements<'a>(
        &self,
        _components: impl IntoIterator<Item = &'a Component>,
    ) -> Cow<'_, RequirementsList<RequestWithOptions>> {
        self.downstream_requirements(|o| o.inheritance() == Inheritance::Strong || o.required)
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

impl OptionValues for EmbeddedPackageSpec {
    fn option_values(&self) -> OptionMap {
        let mut opts = OptionMap::default();
        for opt in self.build.options.iter() {
            // since this is an [Embedded]PackageSpec we can assume that this
            // spec has had all of the options pinned/resolved.
            opts.insert(opt.full_name().to_owned(), opt.get_value(None));
        }
        opts
    }
}

impl Versioned for EmbeddedPackageSpec {
    fn compat(&self) -> &Compat {
        &self.compat
    }
}

impl Satisfy<PkgRequestWithOptions> for EmbeddedPackageSpec {
    fn check_satisfies_request(&self, pkg_request: &PkgRequestWithOptions) -> Compatibility {
        check_package_spec_satisfies_pkg_request(self, pkg_request)
    }
}
