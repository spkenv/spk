// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::borrow::Cow;
use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use spk_schema_foundation::IsDefault;
use spk_schema_foundation::ident::{
    BuildIdent,
    InclusionPolicy,
    NameAndValue,
    PkgRequest,
    RangeIdent,
    Request,
    RequestedBy,
    VarRequest,
    VersionIdent,
};
use spk_schema_foundation::ident_build::{Build, BuildId};
use spk_schema_foundation::ident_component::Component;
use spk_schema_foundation::name::{OptName, PkgName};
use spk_schema_foundation::option_map::{HOST_OPTIONS, OptionMap};
use spk_schema_foundation::spec_ops::{HasVersion, Named, Versioned};
use spk_schema_foundation::version::Version;
use spk_schema_foundation::version_range::VersionFilter;

use crate::foundation::version::Compat;
use crate::ident::is_false;
use crate::metadata::Meta;
use crate::option::VarOpt;
use crate::v0::{Spec, TestSpec};
use crate::{
    BuildEnv,
    BuildSpec,
    ComponentSpec,
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

#[derive(Debug, Clone, Hash, PartialEq, Eq, Deserialize, Serialize)]
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
    #[serde(default, skip_serializing_if = "RequirementsList::is_default")]
    pub requirements: RequirementsList<PlatformRequirement>,
}

impl Platform {
    pub fn build_options(&self) -> Cow<'_, [Opt]> {
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
        // there are no variants to platform builds as they simply capture a single state
        // of the requirements
        BuildSpec::default().build_digest(self.name(), variant)
    }

    fn default_variants(&self, _options: &OptionMap) -> Cow<'_, Vec<Self::Variant>> {
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

            requirements.insert_or_replace(Request::Pkg(PkgRequest::from_ident(
                base.clone().into_any_ident(None),
                RequestedBy::BinaryBuild(self.ident().to_build_ident(Build::BuildId(build_digest))),
            )));
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
            let build_cmpt = spec
                .install
                .components
                .get_or_insert_with(Component::Build, ComponentSpec::default_build);
            apply_inherit_from_base_component(build_cmpt, Component::Build, base);
            let run_cmpt = spec
                .install
                .components
                .get_or_insert_with(Component::Run, ComponentSpec::default_run);
            apply_inherit_from_base_component(run_cmpt, Component::Run, base);
        }

        for requirement in requirements.iter() {
            requirement.update_spec_for_binary_build(&mut spec, build_env)?;
        }

        spec.generate_binary_build(variant, build_env)
    }

    fn metadata(&self) -> &Meta {
        &self.meta
    }
}

fn apply_inherit_from_base_component(
    cmpt: &mut ComponentSpec,
    inherit: Component,
    base: impl Package,
) {
    for requirement in base.runtime_requirements().iter() {
        cmpt.requirements.insert_or_replace(requirement.clone());
    }
    let Some(base_cmpt) = base.components().get(inherit) else {
        return;
    };
    for requirement in base_cmpt.requirements.iter() {
        cmpt.requirements.insert_or_replace(requirement.clone());
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum PlatformRequirement {
    Pkg(PlatformPkgRequirement),
    Var(PlatformVarRequirement),
}

impl Named<OptName> for PlatformRequirement {
    fn name(&self) -> &OptName {
        match self {
            Self::Pkg(p) => p.name(),
            Self::Var(v) => v.name(),
        }
    }
}

impl PlatformRequirement {
    /// Update the given spec with the requirements for this platform.
    fn update_spec_for_binary_build<E, P>(
        &self,
        spec: &mut crate::v0::Spec<VersionIdent>,
        build_env: &E,
    ) -> Result<()>
    where
        E: BuildEnv<Package = P>,
        P: Package,
    {
        match self {
            Self::Pkg(p) => p.update_spec_for_binary_build(spec, build_env),
            Self::Var(v) => v.update_spec_for_binary_build(spec, build_env),
        }
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlatformPkgRequirement {
    pkg: VersionIdent,
    #[serde(
        default,
        with = "value_or_false",
        skip_serializing_if = "Option::is_none"
    )]
    at_build: Option<Override<VersionFilter>>,
    #[serde(
        default,
        with = "value_or_false",
        skip_serializing_if = "Option::is_none"
    )]
    at_runtime: Option<Override<VersionFilter>>,
}

impl Named<OptName> for PlatformPkgRequirement {
    fn name(&self) -> &OptName {
        self.pkg.name().as_opt_name()
    }
}

impl PlatformPkgRequirement {
    /// Update the given spec with the requirements for this platform.
    fn update_spec_for_binary_build<E, P>(
        &self,
        spec: &mut crate::v0::Spec<VersionIdent>,
        _build_env: &E,
    ) -> Result<()>
    where
        E: BuildEnv<Package = P>,
        P: Package,
    {
        let build_component = spec
            .install
            .components
            .get_or_insert_with(Component::Build, ComponentSpec::default_build);
        match &self.at_build {
            None => {}
            Some(Override::Remove) => build_component.requirements.remove_all(self.name()),
            Some(Override::Replace(v)) => {
                build_component
                    .requirements
                    .insert_or_replace(Request::Pkg(PkgRequest {
                        pkg: RangeIdent {
                            repository_name: None,
                            name: self.pkg.name().to_owned(),
                            version: v.clone(),
                            components: Default::default(),
                            build: None,
                        },
                        prerelease_policy: None,
                        inclusion_policy: InclusionPolicy::IfAlreadyPresent,
                        pin: None,
                        pin_policy: spk_schema_foundation::ident::PinPolicy::Required,
                        required_compat: None,
                        requested_by: Default::default(),
                    }));
            }
        }

        let runtime_component = spec
            .install
            .components
            .get_or_insert_with(Component::Run, ComponentSpec::default_run);
        match &self.at_runtime {
            None => {}
            Some(Override::Remove) => runtime_component.requirements.remove_all(self.name()),
            Some(Override::Replace(v)) => {
                runtime_component
                    .requirements
                    .insert_or_replace(Request::Pkg(PkgRequest {
                        pkg: RangeIdent {
                            repository_name: None,
                            name: self.pkg.name().to_owned(),
                            version: v.clone(),
                            components: Default::default(),
                            build: None,
                        },
                        prerelease_policy: None,
                        inclusion_policy: InclusionPolicy::IfAlreadyPresent,
                        pin: None,
                        pin_policy: spk_schema_foundation::ident::PinPolicy::Required,
                        required_compat: None,
                        requested_by: Default::default(),
                    }));
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlatformVarRequirement {
    var: NameAndValue,
    #[serde(
        default,
        with = "value_or_false",
        skip_serializing_if = "Option::is_none"
    )]
    at_build: Option<Override<String>>,
    #[serde(
        default,
        with = "value_or_false",
        skip_serializing_if = "Option::is_none"
    )]
    at_runtime: Option<Override<String>>,
}

impl Named<OptName> for PlatformVarRequirement {
    fn name(&self) -> &OptName {
        &self.var.0
    }
}

impl PlatformVarRequirement {
    /// Update the given spec with the requirements for this platform.
    fn update_spec_for_binary_build<E, P>(
        &self,
        spec: &mut crate::v0::Spec<VersionIdent>,
        _build_env: &E,
    ) -> Result<()>
    where
        E: BuildEnv<Package = P>,
        P: Package,
    {
        // we don't have enough context to make a meaningful description so
        // instead choose to save space and put nothing
        const DESCRIPTION: Option<String> = None;

        let build_component = spec
            .install
            .components
            .get_or_insert_with(Component::Build, ComponentSpec::default_build);
        match &self.at_build {
            None => {}
            Some(Override::Remove) => build_component.requirements.remove_all(self.name()),
            Some(Override::Replace(v)) => {
                build_component
                    .requirements
                    .insert_or_replace(Request::Var(VarRequest {
                        var: self.var.0.clone(),
                        value: spk_schema_foundation::ident::PinnableValue::Pinned(Arc::from(
                            v.as_str(),
                        )),
                        description: DESCRIPTION,
                    }));
            }
        }

        let runtime_component = spec
            .install
            .components
            .get_or_insert_with(Component::Run, ComponentSpec::default_run);
        match &self.at_runtime {
            None => {}
            Some(Override::Remove) => runtime_component.requirements.remove_all(self.name()),
            Some(Override::Replace(v)) => {
                runtime_component
                    .requirements
                    .insert_or_replace(Request::Var(VarRequest {
                        var: self.var.0.clone(),
                        value: spk_schema_foundation::ident::PinnableValue::Pinned(Arc::from(
                            v.as_str(),
                        )),
                        description: DESCRIPTION,
                    }));
            }
        }

        Ok(())
    }
}

/// Overrides the value of some request within a platform
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum Override<R> {
    Remove,
    Replace(R),
}

mod value_or_false {
    use std::fmt::Display;
    use std::marker::PhantomData;

    use super::Override;

    pub fn deserialize<'de, D, R>(deserializer: D) -> Result<Option<Override<R>>, D::Error>
    where
        D: serde::de::Deserializer<'de>,
        R: std::str::FromStr,
        R::Err: std::fmt::Display,
    {
        struct ValueOrFalseVisitor<R>(PhantomData<fn() -> R>);

        impl<R> serde::de::Visitor<'_> for ValueOrFalseVisitor<R>
        where
            R: std::str::FromStr,
            R::Err: std::fmt::Display,
        {
            type Value = Option<Override<R>>;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                "a string value or 'false'".fmt(formatter)
            }

            fn visit_bool<E>(self, v: bool) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                if !v {
                    return Ok(Some(Override::Remove));
                }
                Err(serde::de::Error::invalid_value(
                    serde::de::Unexpected::Bool(v),
                    &self,
                ))
            }

            fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                self.visit_str(&v.to_string())
            }

            fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                self.visit_str(&v.to_string())
            }

            fn visit_f64<E>(self, v: f64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                self.visit_str(&v.to_string())
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                v.parse()
                    .map_err(serde::de::Error::custom)
                    .map(Override::Replace)
                    .map(Some)
            }
        }

        deserializer.deserialize_any(ValueOrFalseVisitor(PhantomData))
    }

    pub fn serialize<S, R>(
        value: &Option<Override<R>>,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
        R: serde::ser::Serialize,
    {
        match value {
            None => serializer.serialize_unit(),
            Some(Override::Remove) => serializer.serialize_bool(false),
            Some(Override::Replace(v)) => v.serialize(serializer),
        }
    }
}
