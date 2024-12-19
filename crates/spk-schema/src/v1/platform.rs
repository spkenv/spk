// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::borrow::Cow;
use std::path::Path;

use serde::{Deserialize, Serialize};
use spk_schema_foundation::ident_build::{Build, BuildId};
use spk_schema_foundation::name::PkgName;
use spk_schema_foundation::option_map::{OptionMap, HOST_OPTIONS};
use spk_schema_foundation::spec_ops::{HasVersion, Named, Versioned};
use spk_schema_foundation::version::Version;
use spk_schema_foundation::IsDefault;
use spk_schema_ident::{BuildIdent, PkgRequest, Request, RequestedBy, VersionIdent};

use crate::foundation::version::Compat;
use crate::ident::is_false;
use crate::metadata::Meta;
use crate::option::VarOpt;
use crate::v0::{Spec, TestSpec};
use crate::{
    BuildEnv,
    BuildSpec,
    Deprecate,
    DeprecateMut,
    InputVariant,
    Opt,
    Package,
    Recipe,
    RequirementsList,
    Result,
    RuntimeEnvironment,
    Script,
    TestStage,
    Variant,
};

#[cfg(test)]
#[path = "./platform_test.rs"]
mod platform_test;

#[derive(Debug, Clone, Hash, PartialEq, Eq, Ord, PartialOrd, Deserialize, Serialize)]
pub struct Platform {
    pub platform: VersionIdent,
    #[serde(default, skip_serializing_if = "IsDefault::is_default")]
    pub meta: Meta,
    #[serde(default, skip_serializing_if = "IsDefault::is_default")]
    pub compat: Compat,
    #[serde(default, skip_serializing_if = "is_false")]
    pub deprecated: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub base: Vec<VersionIdent>,
    #[serde(default, skip_serializing_if = "super::RecipeOptionList::is_empty")]
    pub requirements: super::RecipeOptionList,
}

impl Platform {
    pub fn build_options(&self) -> Cow<'_, [Opt]> {
        // Platforms have no build!
        Cow::Borrowed(&[])
    }
}

impl Deprecate for Platform {
    fn is_deprecated(&self) -> bool {
        self.deprecated
    }
}

impl DeprecateMut for Platform {
    fn deprecate(&mut self) -> Result<()> {
        self.deprecated = true;
        Ok(())
    }

    fn undeprecate(&mut self) -> Result<()> {
        self.deprecated = false;
        Ok(())
    }
}

impl HasVersion for Platform {
    fn version(&self) -> &Version {
        self.platform.version()
    }
}

impl Named for Platform {
    fn name(&self) -> &PkgName {
        self.platform.name()
    }
}

impl RuntimeEnvironment for Platform {
    fn runtime_environment(&self) -> &[crate::EnvOp] {
        // Platforms don't have any EnvOps
        &[]
    }
}

impl Versioned for Platform {
    fn compat(&self) -> &Compat {
        &self.compat
    }
}

impl Recipe for Platform {
    type Output = Spec<BuildIdent>;
    type Variant = crate::v0::Variant;
    type Test = TestSpec;

    fn ident(&self) -> &VersionIdent {
        &self.platform
    }

    #[inline]
    fn build_digest<V>(&self, variant: &V) -> Result<BuildId>
    where
        V: Variant,
    {
        // Platforms have no build!
        BuildSpec::default().build_digest(self.name(), variant)
    }

    fn default_variants(&self, _options: &OptionMap) -> Cow<'_, Vec<Self::Variant>> {
        // Platforms have no build!
        Cow::Owned(vec![Self::Variant::default()])
    }

    fn resolve_options<V>(&self, _variant: &V) -> Result<OptionMap>
    where
        V: Variant,
    {
        Ok(OptionMap::default())
    }

    fn get_build_requirements<V>(&self, variant: &V) -> Result<Cow<'_, RequirementsList>>
    where
        V: Variant,
    {
        let mut requirements = RequirementsList::default();

        for base in self.base.iter() {
            let build_digest = self.build_digest(variant)?;

            requirements.insert_or_merge(Request::Pkg(PkgRequest::from_ident(
                base.clone().into_any_ident(None),
                RequestedBy::BinaryBuild(self.ident().to_build_ident(Build::BuildId(build_digest))),
            )))?;
        }

        Ok(Cow::Owned(requirements))
    }

    fn get_tests<V>(&self, _stage: TestStage, _variant: &V) -> Result<Vec<Self::Test>>
    where
        V: Variant,
    {
        Ok(Vec::new())
    }

    fn generate_source_build(&self, _root: &Path) -> Result<Self::Output> {
        Ok(Spec::new(
            self.platform.clone().into_build_ident(Build::Source),
        ))
    }

    fn generate_binary_build<V, E, P>(&self, variant: &V, build_env: &E) -> Result<Self::Output>
    where
        V: InputVariant,
        E: BuildEnv<Package = P>,
        P: Package,
    {
        let Self {
            platform,
            meta,
            compat,
            deprecated: _deprecated,
            base,
            requirements,
        } = self;

        // Translate the platform spec into a "normal" recipe and delegate to
        // that recipe's generate_binary_build method.
        let mut spec = crate::v0::Spec::new(platform.clone());
        spec.compat = compat.clone();
        spec.meta = meta.clone();

        // Platforms have no sources
        spec.sources = Vec::new();

        // Supply a safe no-op build script and standard host/os
        // related option names but leave the values for the later
        // steps of building to fill in.
        let mut build_host_options = Vec::new();
        for (name, _value) in HOST_OPTIONS.get()?.iter() {
            build_host_options.push(Opt::Var(VarOpt::new(name)?));
        }
        spec.build.script = Script::new([""]);
        spec.build.options = build_host_options;

        // Add base requirements, if any, first.
        for base in base.iter() {
            let build_env = build_env.build_env();
            let base = build_env
                .iter()
                .find(|package| package.name() == base.name())
                .ok_or_else(|| {
                    crate::Error::String(format!(
                        "base platform '{}' not found in build environment",
                        base.name()
                    ))
                })?;
            for requirement in base.runtime_requirements().iter() {
                spec.install
                    .requirements
                    .insert_or_replace(requirement.clone());
            }
        }

        let opts = requirements.to_package_options();

        spec.generate_binary_build(variant, build_env)
    }

    fn metadata(&self) -> &Meta {
        &self.meta
    }
}
