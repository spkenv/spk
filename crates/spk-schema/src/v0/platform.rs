// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::borrow::Cow;
use std::collections::HashSet;
use std::ops::{Deref, DerefMut};
use std::path::Path;

use serde::{Deserialize, Serialize};
use spk_schema_foundation::ident_build::Build;
use spk_schema_foundation::name::PkgName;
use spk_schema_foundation::option_map::{host_options, OptionMap, Stringified};
use spk_schema_foundation::spec_ops::{HasVersion, Named, Versioned};
use spk_schema_foundation::version::Version;
use spk_schema_ident::{
    BuildIdent,
    Ident,
    InclusionPolicy,
    PkgRequest,
    Request,
    RequestedBy,
    VersionIdent,
};

use super::{Spec, TestSpec};
use crate::foundation::version::Compat;
use crate::ident::is_false;
use crate::metadata::Meta;
use crate::option::VarOpt;
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
    Script,
    TestStage,
    Variant,
};

#[cfg(test)]
#[path = "./platform_test.rs"]
mod platform_test;

#[derive(Debug, Clone, Hash, PartialEq, Eq, Ord, PartialOrd, Serialize)]
pub struct PlatformRequirementsPatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    add: Option<RequirementsList>,
    #[serde(skip_serializing_if = "Option::is_none")]
    remove: Option<RequirementsList>,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, Ord, PartialOrd, Serialize)]
#[serde(untagged)]
pub enum PlatformRequirements {
    BareAdd(RequirementsList),
    Patch(PlatformRequirementsPatch),
}

impl PlatformRequirements {
    /// Handle an "add" operation, adding a request to the given list.
    ///
    /// The request's attributes may be modified to conform to the expected
    /// behavior for platform requirements.
    fn process_add(dest: &mut RequirementsList, add: &RequirementsList) {
        for request in add.iter() {
            let mut request = request.clone();
            if let Request::Pkg(pkg) = &mut request {
                pkg.inclusion_policy = InclusionPolicy::IfAlreadyPresent;
            };
            dest.insert_or_replace(request);
        }
    }

    /// Update the given spec with the requirements for this platform.
    fn update_spec_for_binary_build<E, P>(
        &self,
        spec: &mut super::Spec<VersionIdent>,
        _build_env: &E,
    ) -> Result<()>
    where
        E: BuildEnv<Package = P>,
        P: Package,
    {
        match self {
            PlatformRequirements::BareAdd(add) => {
                PlatformRequirements::process_add(&mut spec.install.requirements, add);
            }
            PlatformRequirements::Patch(patch) => {
                // Removes are performed first; an inherited request can be
                // removed and re-added with a different set of components.
                if let Some(remove) = &patch.remove {
                    for request in remove.iter() {
                        spec.install.requirements.remove_all(request.name());
                    }
                }

                if let Some(add) = &patch.add {
                    PlatformRequirements::process_add(&mut spec.install.requirements, add);
                }
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, Ord, PartialOrd, Serialize)]
pub struct Platform<Ident> {
    pub platform: Ident,
    #[serde(default, skip_serializing_if = "Meta::is_default")]
    pub meta: Meta,
    #[serde(default, skip_serializing_if = "Compat::is_default")]
    pub compat: Compat,
    #[serde(default, skip_serializing_if = "is_false")]
    pub deprecated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<Ident>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requirements: Option<PlatformRequirements>,
}

impl<Ident> Deprecate for Platform<Ident> {
    fn is_deprecated(&self) -> bool {
        self.deprecated
    }
}

impl<Ident> DeprecateMut for Platform<Ident> {
    fn deprecate(&mut self) -> Result<()> {
        self.deprecated = true;
        Ok(())
    }

    fn undeprecate(&mut self) -> Result<()> {
        self.deprecated = false;
        Ok(())
    }
}

impl<Ident: Named> Named for Platform<Ident> {
    fn name(&self) -> &PkgName {
        self.platform.name()
    }
}

impl<Ident: HasVersion> HasVersion for Platform<Ident> {
    fn version(&self) -> &Version {
        self.platform.version()
    }
}

impl<Ident: HasVersion> Versioned for Platform<Ident> {
    fn compat(&self) -> &Compat {
        &self.compat
    }
}

impl Recipe for Platform<VersionIdent> {
    type Output = Spec<BuildIdent>;
    type Variant = super::Variant;
    type Test = TestSpec;

    fn ident(&self) -> &VersionIdent {
        &self.platform
    }

    fn default_variants(&self) -> Cow<'_, Vec<Self::Variant>> {
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
        let options = self.resolve_options(variant)?;
        let build_digest = Build::Digest(options.digest());

        let mut requirements = RequirementsList::default();

        if let Some(base) = self.base.as_ref() {
            requirements.insert_or_merge(Request::Pkg(PkgRequest::from_ident(
                base.clone().into_any(None),
                RequestedBy::BinaryBuild(self.ident().to_build(build_digest.clone())),
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
        Ok(Spec::new(self.platform.clone().into_build(Build::Source)))
    }

    fn generate_binary_build<V, E, P>(&self, variant: &V, build_env: &E) -> Result<Self::Output>
    where
        V: InputVariant,
        E: BuildEnv<Package = P>,
        P: Package,
    {
        // Translate the platform spec into a "normal" recipe and delegate to
        // that recipe's generate_binary_build method.
        let mut spec = super::Spec::new(self.platform.clone());
        spec.compat = self.compat.clone();
        spec.meta = self.meta.clone();

        // Platforms have no sources
        spec.sources = Vec::new();

        // Supply a safe no-op build script and standard host/os
        // related option names but leave the values for the later
        // steps of building to fill in.
        let mut build_host_options = Vec::new();
        for (name, _value) in host_options()?.iter() {
            build_host_options.push(Opt::Var(VarOpt::new(name)?));
        }
        spec.build = BuildSpec {
            script: Script::new(["true"]),
            options: build_host_options,
            ..Default::default()
        };

        // Add base requirements, if any, first.
        if let Some(base) = self.base.as_ref() {
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

        if let Some(requirements) = self.requirements.as_ref() {
            requirements.update_spec_for_binary_build(&mut spec, build_env)?;
        }

        spec.generate_binary_build(variant, build_env)
    }
}

// A private visitor struct that may be extended to aid linting in future.
// It is currently identical to the PlatformRequirementsPatch struct.
// If it does not change or gain extra methods when linting is added,
// then it can probably be removed and the public PlatformRequirementsPatch
// used in its place.
// TODO: update this when linting/warning support is added
#[derive(Default)]
struct PlatformRequirementsPatchVisitor {
    add: Option<RequirementsList>,
    remove: Option<RequirementsList>,
}

impl From<PlatformRequirementsPatchVisitor> for PlatformRequirementsPatch {
    fn from(visitor: PlatformRequirementsPatchVisitor) -> Self {
        Self {
            add: visitor.add,
            remove: visitor.remove,
        }
    }
}

impl From<PlatformRequirementsPatch> for PlatformRequirementsPatchVisitor {
    fn from(patch: PlatformRequirementsPatch) -> Self {
        Self {
            add: patch.add,
            remove: patch.remove,
        }
    }
}

#[derive(Default)]
struct PlatformRequirementsVisitor {
    bare_add: Option<RequirementsList>,
    patch: Option<PlatformRequirementsPatchVisitor>,
}

impl From<PlatformRequirementsVisitor> for PlatformRequirements {
    fn from(visitor: PlatformRequirementsVisitor) -> Self {
        if let Some(add) = visitor.bare_add {
            Self::BareAdd(add)
        } else if let Some(patch) = visitor.patch {
            Self::Patch(patch.into())
        } else {
            Self::BareAdd(RequirementsList::default())
        }
    }
}

impl From<PlatformRequirements> for PlatformRequirementsVisitor {
    fn from(requirements: PlatformRequirements) -> Self {
        match requirements {
            PlatformRequirements::BareAdd(add) => Self {
                bare_add: Some(add),
                ..Default::default()
            },
            PlatformRequirements::Patch(patch) => Self {
                patch: Some(patch.into()),
                ..Default::default()
            },
        }
    }
}

impl<'de> serde::de::Visitor<'de> for PlatformRequirementsVisitor {
    type Value = PlatformRequirements;

    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("a platform requirements specification")
    }

    fn visit_map<A>(self, mut map: A) -> std::result::Result<Self::Value, A::Error>
    where
        A: serde::de::MapAccess<'de>,
    {
        let mut patch = PlatformRequirementsPatchVisitor::default();
        while let Some(key) = map.next_key::<Stringified>()? {
            match key.as_str() {
                "add" => {
                    patch.add = Some(map.next_value::<RequirementsList>()?);
                }
                "remove" => {
                    patch.remove = Some(map.next_value::<RequirementsList>()?);
                }
                _ => {
                    // ignore any unrecognized field, but consume the value anyway
                    // TODO: could we warn about fields that look like typos?
                    map.next_value::<serde::de::IgnoredAny>()?;
                }
            }
        }

        Ok(PlatformRequirements::Patch(patch.into()))
    }

    fn visit_seq<A>(self, mut seq: A) -> std::result::Result<Self::Value, A::Error>
    where
        A: serde::de::SeqAccess<'de>,
    {
        let mut requirements = RequirementsList::default();

        // XXX: This duplicates the logic in RequirementsList::deserialize,
        // but without size_hints because there's no constructor for
        // RequirementsList where we can provide our own Vec.
        let mut requirement_names = HashSet::new();
        while let Some(request) = seq.next_element::<Request>()? {
            let name = request.name();
            if !requirement_names.insert(name.to_owned()) {
                return Err(serde::de::Error::custom(format!(
                    "found multiple platform requirements for '{name}'"
                )));
            }
            requirements.insert_or_replace(request);
        }

        Ok(PlatformRequirements::BareAdd(requirements))
    }
}

impl<'de> Deserialize<'de> for PlatformRequirementsVisitor {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        Ok(deserializer
            .deserialize_any(PlatformRequirementsVisitor::default())?
            .into())
    }
}

struct PlatformVisitor<B, T> {
    platform: Option<Ident<B, T>>,
    base: Option<Ident<B, T>>,
    meta: Option<Meta>,
    compat: Option<Compat>,
    deprecated: Option<bool>,
    requirements: Option<PlatformRequirementsVisitor>,
}

impl<B, T> Default for PlatformVisitor<B, T> {
    fn default() -> Self {
        Self {
            platform: None,
            base: None,
            meta: None,
            compat: None,
            deprecated: None,
            requirements: None,
        }
    }
}

impl<'de, B, T> serde::de::Visitor<'de> for PlatformVisitor<B, T>
where
    Ident<B, T>: serde::de::DeserializeOwned,
{
    type Value = Platform<Ident<B, T>>;

    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("a platform specification")
    }

    fn visit_map<A>(mut self, mut map: A) -> std::result::Result<Self::Value, A::Error>
    where
        A: serde::de::MapAccess<'de>,
    {
        while let Some(key) = map.next_key::<Stringified>()? {
            match key.as_str() {
                "platform" => self.platform = Some(map.next_value::<Ident<B, T>>()?),
                "base" => self.base = Some(map.next_value::<Ident<B, T>>()?),
                "meta" => self.meta = Some(map.next_value::<Meta>()?),
                "compat" => self.compat = Some(map.next_value::<Compat>()?),
                "deprecated" => self.deprecated = Some(map.next_value::<bool>()?),
                "requirements" => {
                    self.requirements = Some(map.next_value::<PlatformRequirementsVisitor>()?)
                }
                _ => {
                    // ignore any unrecognized field, but consume the value anyway
                    // TODO: could we warn about fields that look like typos?
                    map.next_value::<serde::de::IgnoredAny>()?;
                }
            }
        }

        let platform = self
            .platform
            .take()
            .ok_or_else(|| serde::de::Error::missing_field("platform"))?;

        Ok(Platform {
            meta: self.meta.take().unwrap_or_default(),
            compat: self.compat.take().unwrap_or_default(),
            deprecated: self.deprecated.take().unwrap_or_default(),
            platform,
            base: self.base.take(),
            requirements: self.requirements.take().map(Into::into),
        })
    }
}

impl<'de> Deserialize<'de> for Platform<VersionIdent>
where
    VersionIdent: serde::de::DeserializeOwned,
{
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        deserializer.deserialize_map(PlatformVisitor::default())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct BuiltPlatform(Spec<BuildIdent>);

impl Deref for BuiltPlatform {
    type Target = Spec<BuildIdent>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for BuiltPlatform {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Deprecate for BuiltPlatform {
    fn is_deprecated(&self) -> bool {
        self.0.is_deprecated()
    }
}

impl DeprecateMut for BuiltPlatform {
    fn deprecate(&mut self) -> Result<()> {
        self.0.deprecate()
    }

    fn undeprecate(&mut self) -> Result<()> {
        self.0.undeprecate()
    }
}

impl Package for BuiltPlatform {
    type Package = super::Spec<BuildIdent>;

    fn ident(&self) -> &BuildIdent {
        self.0.ident()
    }

    fn option_values(&self) -> OptionMap {
        self.0.option_values()
    }

    fn sources(&self) -> &Vec<crate::SourceSpec> {
        self.0.sources()
    }

    fn embedded(&self) -> &crate::EmbeddedPackagesList {
        self.0.embedded()
    }

    fn embedded_as_packages(
        &self,
    ) -> std::result::Result<
        Vec<(
            Self::Package,
            Option<spk_schema_foundation::ident_component::Component>,
        )>,
        &str,
    > {
        self.0.embedded_as_packages()
    }

    fn components(&self) -> &crate::ComponentSpecList {
        self.0.components()
    }

    fn runtime_environment(&self) -> &Vec<crate::EnvOp> {
        self.0.runtime_environment()
    }

    fn get_build_requirements(&self) -> crate::Result<Cow<'_, RequirementsList>> {
        self.0.get_build_requirements()
    }

    fn runtime_requirements(&self) -> Cow<'_, RequirementsList> {
        self.0.runtime_requirements()
    }

    fn downstream_build_requirements<'a>(
        &self,
        components: impl IntoIterator<Item = &'a spk_schema_foundation::ident_component::Component>,
    ) -> Cow<'_, RequirementsList> {
        self.0.downstream_build_requirements(components)
    }

    fn downstream_runtime_requirements<'a>(
        &self,
        components: impl IntoIterator<Item = &'a spk_schema_foundation::ident_component::Component>,
    ) -> Cow<'_, RequirementsList> {
        self.0.downstream_runtime_requirements(components)
    }

    fn validation(&self) -> &crate::ValidationSpec {
        self.0.validation()
    }

    fn build_script(&self) -> String {
        self.0.build_script()
    }

    fn validate_options(
        &self,
        given_options: &OptionMap,
    ) -> spk_schema_foundation::version::Compatibility {
        self.0.validate_options(given_options)
    }
}

impl HasVersion for BuiltPlatform {
    fn version(&self) -> &Version {
        self.0.version()
    }
}

impl Named for BuiltPlatform {
    fn name(&self) -> &PkgName {
        self.0.name()
    }
}

impl Versioned for BuiltPlatform {
    fn compat(&self) -> &Compat {
        self.0.compat()
    }
}
