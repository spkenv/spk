// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::borrow::Cow;
use std::path::Path;

use serde::{Deserialize, Serialize};
use spk_schema_foundation::ident_build::{Build, BuildId};
use spk_schema_foundation::name::PkgName;
use spk_schema_foundation::option_map::{OptionMap, Stringified, HOST_OPTIONS};
use spk_schema_foundation::spec_ops::{HasVersion, Named, Versioned};
use spk_schema_foundation::version::Version;
use spk_schema_ident::{
    BuildIdent,
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
pub struct Platform {
    pub platform: VersionIdent,
    #[serde(default, skip_serializing_if = "Meta::is_default")]
    pub meta: Meta,
    #[serde(default, skip_serializing_if = "Compat::is_default")]
    pub compat: Compat,
    #[serde(default, skip_serializing_if = "is_false")]
    pub deprecated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<VersionIdent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requirements: Option<PlatformRequirements>,
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

impl Named for Platform {
    fn name(&self) -> &PkgName {
        self.platform.name()
    }
}

impl HasVersion for Platform {
    fn version(&self) -> &Version {
        self.platform.version()
    }
}

impl Versioned for Platform {
    fn compat(&self) -> &Compat {
        &self.compat
    }
}

impl Recipe for Platform {
    type Output = Spec<BuildIdent>;
    type Variant = super::Variant;
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

        if let Some(base) = self.base.as_ref() {
            let build_digest = self.build_digest(variant)?;

            requirements.insert_or_merge(Request::Pkg(PkgRequest::from_ident(
                base.clone().into_any(None),
                RequestedBy::BinaryBuild(self.ident().to_build(Build::BuildId(build_digest))),
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
        let mut spec = super::Spec::new(platform.clone());
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
        if let Some(base) = base.as_ref() {
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

        if let Some(requirements) = requirements.as_ref() {
            requirements.update_spec_for_binary_build(&mut spec, build_env)?;
        }

        spec.generate_binary_build(variant, build_env)
    }

    fn metadata(&self) -> &Meta {
        &self.meta
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

    fn visit_seq<A>(self, seq: A) -> std::result::Result<Self::Value, A::Error>
    where
        A: serde::de::SeqAccess<'de>,
    {
        let requirements =
            RequirementsList::deserialize(serde::de::value::SeqAccessDeserializer::new(seq))?;

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

#[derive(Default)]
struct PlatformVisitor {
    platform: Option<VersionIdent>,
    base: Option<VersionIdent>,
    meta: Option<Meta>,
    compat: Option<Compat>,
    deprecated: Option<bool>,
    requirements: Option<PlatformRequirementsVisitor>,
}

impl<'de> serde::de::Visitor<'de> for PlatformVisitor {
    type Value = Platform;

    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("a platform specification")
    }

    fn visit_map<A>(mut self, mut map: A) -> std::result::Result<Self::Value, A::Error>
    where
        A: serde::de::MapAccess<'de>,
    {
        while let Some(key) = map.next_key::<Stringified>()? {
            match key.as_str() {
                "platform" => self.platform = Some(map.next_value::<VersionIdent>()?),
                "base" => self.base = Some(map.next_value::<VersionIdent>()?),
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

impl<'de> Deserialize<'de> for Platform
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
