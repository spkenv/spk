use std::borrow::Cow;

// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use serde::{Deserialize, Serialize};
use spk_schema_foundation::ident_build::EmbeddedSource;
use spk_schema_foundation::ident_component::Component;
use spk_schema_foundation::spec_ops::{HasVersion, Named, Versioned};
use spk_schema_ident::{AnyIdent, BuildIdent};

use super::{BuildSpec, InstallSpec};
use crate::foundation::ident_build::Build;
use crate::prelude::*;
use crate::Result;

#[cfg(test)]
#[path = "./embedded_package_test.rs"]
mod embedded_package_test;

/// A set of packages that are embedded/provided by another.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct EmbeddedPackage(super::Package);

impl EmbeddedPackage {
    pub fn into_inner(self) -> super::Package {
        self.0
    }
}

impl Named for EmbeddedPackage {
    fn name(&self) -> &spk_schema_foundation::name::PkgName {
        self.0.name()
    }
}

impl HasVersion for EmbeddedPackage {
    fn version(&self) -> &spk_schema_foundation::version::Version {
        self.0.version()
    }
}

impl Versioned for EmbeddedPackage {
    fn compat(&self) -> &spk_schema_foundation::version::Compat {
        self.0.compat()
    }
}

impl Deprecate for EmbeddedPackage {
    fn is_deprecated(&self) -> bool {
        self.0.is_deprecated()
    }
}

impl DeprecateMut for EmbeddedPackage {
    fn deprecate(&mut self) -> Result<()> {
        self.0.deprecate()
    }

    fn undeprecate(&mut self) -> Result<()> {
        self.0.undeprecate()
    }
}

impl crate::Package for EmbeddedPackage {
    type EmbeddedStub = <super::Package as crate::Package>::EmbeddedStub;

    fn ident(&self) -> &BuildIdent {
        self.0.ident()
    }

    fn option_values(&self) -> spk_schema_foundation::option_map::OptionMap {
        self.0.option_values()
    }

    fn sources(&self) -> &Vec<crate::SourceSpec> {
        self.0.sources()
    }

    fn embedded<'a>(
        &self,
        components: impl IntoIterator<Item = &'a spk_schema_foundation::ident_component::Component>,
    ) -> Vec<Self::EmbeddedStub> {
        self.0.embedded(components)
    }

    fn components(&self) -> std::borrow::Cow<'_, crate::ComponentSpecList<Self::EmbeddedStub>> {
        self.0.components()
    }

    fn runtime_environment(&self) -> &Vec<crate::EnvOp> {
        self.0.runtime_environment()
    }

    fn runtime_requirements<'a>(
        &self,
        components: impl IntoIterator<Item = &'a Component>,
    ) -> Cow<'_, crate::RequirementsList> {
        self.0.runtime_requirements(components)
    }

    fn downstream_requirements<'a>(
        &self,
        components: impl IntoIterator<Item = &'a spk_schema_foundation::ident_component::Component>,
    ) -> Cow<'_, crate::RequirementsList> {
        self.0.downstream_requirements(components)
    }

    fn validation(&self) -> &crate::ValidationSpec {
        self.0.validation()
    }

    fn build_script(&self) -> Cow<'_, String> {
        self.0.build_script()
    }

    fn validate_options(
        &self,
        given_options: &spk_schema_foundation::option_map::OptionMap,
    ) -> spk_schema_foundation::version::Compatibility {
        self.0.validate_options(given_options)
    }
}

impl<'de> Deserialize<'de> for EmbeddedPackage {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let mut default_build_spec = BuildSpec::default();
        let mut default_install_spec = InstallSpec::default();
        let embedded = super::Spec::<AnyIdent>::deserialize(deserializer)?;
        default_build_spec.options = embedded.build.options.clone();
        if default_build_spec != embedded.build {
            return Err(serde::de::Error::custom(
                "embedded packages can only specify build.options",
            ));
        }
        default_install_spec.components = embedded.install.components.clone();
        if default_install_spec != embedded.install {
            return Err(serde::de::Error::custom(
                "embedded packages can only specify install.components",
            ));
        }
        let embedded = match embedded.pkg.build() {
            None | Some(Build::Embedded(EmbeddedSource::Unknown)) => {
                embedded.map_ident(|i| i.to_build(Build::Embedded(EmbeddedSource::Unknown)))
            }
            Some(_) => {
                return Err(serde::de::Error::custom(format!(
                    "embedded package should not specify a build, got: {}",
                    embedded.pkg
                )));
            }
        };
        Ok(EmbeddedPackage(embedded))
    }
}
