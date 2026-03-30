// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::borrow::Cow;
use std::cmp::Ordering;
use std::str::FromStr;
use std::sync::Arc;

use arc_swap::{ArcSwap, ArcSwapOption};
use serde::Serialize;
use spk_schema_foundation::IsDefault;
use spk_schema_foundation::ident::{
    BuildIdent,
    PinnedRequest,
    PinnedValue,
    PkgRequestWithOptions,
    RequestWithOptions,
};
use spk_schema_foundation::ident_build::Build;
use spk_schema_foundation::ident_component::Component;
use spk_schema_foundation::name::OptNameBuf;
use spk_schema_foundation::option_map::{OptFilter, OptionMap};
use spk_schema_foundation::spec_ops::HasBuildIdent;
use spk_schema_foundation::version::{IncompatibleReason, VarOptionProblem};

use super::check_package_spec_satisfies_pkg_request;
use crate::fb_converter::fb_requirements_to_requirements;
use crate::foundation::name::PkgName;
use crate::foundation::spec_ops::prelude::*;
use crate::foundation::version::{Compat, Compatibility, Version};
use crate::ident::{Satisfy, VarRequest};
use crate::package::OptionValues;
use crate::spec::SpecTest;
use crate::v0::EmbeddedPackageSpec;
use crate::{
    BuildOptions,
    ComponentSpec,
    ComponentSpecList,
    Components,
    Deprecate,
    DeprecateMut,
    DownstreamRequirements,
    EmbeddedPackagesList,
    Error,
    Opt,
    Package,
    PackageMut,
    RequirementsList,
    Result,
    RuntimeEnvironment,
    SourceSpec,
    fb_compat_to_compat,
    fb_component_specs_to_component_specs,
    fb_embedded_package_specs_to_embedded_package_specs,
    fb_opt_to_opt,
    fb_opts_to_opts,
};

// A package extracted from an index
#[derive(Debug, Serialize)]
pub struct IndexedPackage {
    build_ident: BuildIdent,
    #[serde(skip)]
    buf: bytes::Bytes,
    offset: usize,

    // Internal caches - used to save parsing/construction time, for
    // now, because there are not flatbuffer equivalents of all the
    // spk objects inside and returned by Spec and the associated traits.
    // The caches can go away if full flatbuffer replacements are added.
    #[serde(skip)]
    cached_compat: ArcSwapOption<Compat>,
    #[serde(skip)]
    cached_embedded: ArcSwap<EmbeddedPackagesList<EmbeddedPackageSpec>>,
    #[serde(skip)]
    cached_component_specs: ArcSwap<ComponentSpecList<ComponentSpec>>,
    #[serde(skip)]
    cached_requirements: ArcSwap<RequirementsList<RequestWithOptions>>,
    #[serde(skip)]
    cached_build_opts: ArcSwap<Vec<Opt>>,
}

impl Clone for IndexedPackage {
    fn clone(&self) -> Self {
        Self {
            build_ident: self.build_ident.clone(),
            buf: self.buf.clone(),
            offset: self.offset,
            cached_compat: ArcSwapOption::new(self.cached_compat.load_full()),
            cached_embedded: ArcSwap::new(self.cached_embedded.load_full()),
            cached_component_specs: ArcSwap::new(self.cached_component_specs.load_full()),
            cached_requirements: ArcSwap::new(self.cached_requirements.load_full()),
            cached_build_opts: ArcSwap::new(self.cached_build_opts.load_full()),
        }
    }
}

impl std::hash::Hash for IndexedPackage {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // Different packages from the same index will have different
        // offsets.
        self.offset.hash(state);
        self.build_ident.hash(state);
        // The index bytes buf and cached fields are skipped
    }
}

impl std::cmp::PartialEq for IndexedPackage {
    fn eq(&self, other: &Self) -> bool {
        // This deliberately ignores the buf field, offset field, and
        // all the caches.
        self.offset == other.offset && self.build_ident == other.build_ident
    }
}

impl std::cmp::Eq for IndexedPackage {}

impl PartialOrd for IndexedPackage {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for IndexedPackage {
    fn cmp(&self, other: &Self) -> Ordering {
        self.build_ident.cmp(&other.build_ident)
    }
}

impl IndexedPackage {
    pub fn new(build_ident: BuildIdent, buf: bytes::Bytes, offset: usize) -> IndexedPackage {
        Self {
            build_ident,
            buf,
            offset,
            cached_compat: ArcSwapOption::from(None),
            cached_embedded: ArcSwap::new(Arc::new(EmbeddedPackagesList::default())),
            // Not using default because that would be a list with
            // build and a run components.
            cached_component_specs: ArcSwap::new(Arc::new(ComponentSpecList::new(Vec::new()))),
            cached_requirements: ArcSwap::new(Arc::new(RequirementsList::default())),
            cached_build_opts: ArcSwap::new(Arc::new(Vec::new())),
        }
    }

    #[inline]
    pub fn build_index(&self) -> spk_proto::BuildIndex<'_> {
        // Safety: we trust that the buffer and offset have been
        // validated, or come from a trusted source.
        unsafe {
            <spk_proto::BuildIndex as flatbuffers::Follow>::follow(&self.buf[..], self.offset)
        }
    }
}

impl BuildOptions for IndexedPackage {
    fn build_options(&self) -> Cow<'_, [Opt]> {
        self.get_build_options()
    }
}

impl Deprecate for IndexedPackage {
    fn is_deprecated(&self) -> bool {
        self.build_index().is_deprecated()
    }
}

impl DeprecateMut for IndexedPackage {
    fn deprecate(&mut self) -> Result<()> {
        Err(Error::SpkIndexedPackageDoesNotImplement(
            "DeprecateMut".to_string(),
            "deprecate".to_string(),
        ))
    }

    fn set_deprecated(&mut self, deprecated: bool) -> Result<()> {
        if deprecated {
            self.deprecate()
        } else {
            self.undeprecate()
        }
    }

    fn undeprecate(&mut self) -> Result<()> {
        Err(Error::SpkIndexedPackageDoesNotImplement(
            "DeprecateMut".to_string(),
            "undeprecate".to_string(),
        ))
    }
}

impl Satisfy<PkgRequestWithOptions> for IndexedPackage {
    fn check_satisfies_request(&self, pkg_request: &PkgRequestWithOptions) -> Compatibility {
        check_package_spec_satisfies_pkg_request(self, pkg_request)
    }
}

impl Satisfy<VarRequest<PinnedValue>> for IndexedPackage {
    fn check_satisfies_request(&self, var_request: &VarRequest<PinnedValue>) -> Compatibility {
        // Copied from V0 Spec<BuildIdent> and slightly adjusted for
        // the flatbuffer data.
        let opt_required = var_request.var.namespace() == Some(self.name());
        let mut opt: Option<&Opt> = None;
        let request_name = &var_request.var;
        let mut option: Opt;

        if let Some(build_opts) = self.build_index().build_options() {
            for fb_opt in build_opts.iter() {
                option = fb_opt_to_opt(&fb_opt);
                let o_name = unsafe { OptNameBuf::from_string(option.base_name().to_string()) };
                if request_name == &o_name {
                    opt = Some(&option);
                    break;
                }
                if *request_name == o_name.with_namespace(self.name()) {
                    opt = Some(&option);
                    break;
                }
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
            Some(Opt::Pkg(opt)) => opt.validate(Some(&*var_request.value)),
            Some(Opt::Var(opt)) => {
                let request_value = &*var_request.value;
                let exact = opt.get_value(Some(request_value));
                if exact.as_deref() == Some(request_value) {
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
                                request_value: request_value.to_string(),
                            },
                        ));
                    };

                    let Ok(request_version) = Version::from_str(request_value) else {
                        return Compatibility::Incompatible(IncompatibleReason::VarOptionMismatch(
                            VarOptionProblem::IncompatibleBuildOptionInvalidVersion {
                                var_request: var_request.var.clone(),
                                base: exact.unwrap_or_default(),
                                request_value: request_value.to_string(),
                            },
                        ));
                    };

                    let result = compat.is_binary_compatible(&base_version, &request_version);
                    if let Compatibility::Incompatible(incompatible) = result {
                        return Compatibility::Incompatible(IncompatibleReason::VarOptionMismatch(
                            VarOptionProblem::IncompatibleBuildOptionWithContext {
                                var_request: var_request.var.clone(),
                                exact: exact.unwrap_or_else(|| "None".to_string()),
                                request_value: request_value.to_string(),
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
                        request_value: request_value.to_string(),
                    },
                ))
            }
        }
    }
}

impl HasVersion for IndexedPackage {
    fn version(&self) -> &Version {
        self.build_ident.version()
    }
}

impl HasBuild for IndexedPackage {
    fn build(&self) -> &Build {
        self.build_ident.build()
    }
}

impl HasBuildIdent for IndexedPackage {
    fn build_ident(&self) -> &BuildIdent {
        &self.build_ident
    }
}

impl Named for IndexedPackage {
    fn name(&self) -> &PkgName {
        self.build_ident.name()
    }
}

impl RuntimeEnvironment for IndexedPackage {
    fn runtime_environment(&self) -> &[crate::EnvOp] {
        let err = Error::SpkIndexedPackageDoesNotImplement(
            "RuntimeEnvironment".to_string(),
            "runtime_environment".to_string(),
        );
        // TODO: should this change the return value, e.g. to
        // Result<&[crate::EnvOp]>, update all the caller's handling,
        // and return an error for this implementation?
        unreachable!("{err}");
    }
}

impl Versioned for IndexedPackage {
    fn compat(&self) -> Cow<'_, Compat> {
        if self.cached_compat.load().is_none() {
            // The compat hasn't been read and decoded yet
            let compat = fb_compat_to_compat(self.build_index().compat());
            self.cached_compat.store(Some(Arc::new(compat)))
        }
        // The unwrap should be safe because is_none() was checked
        // above and the value updated to some compat value.
        let c = (**self.cached_compat.load().as_ref().unwrap()).clone();
        Cow::Owned(c)
    }
}

impl Components for IndexedPackage {
    type ComponentSpecT = ComponentSpec;

    fn components(&self) -> Cow<'_, ComponentSpecList<Self::ComponentSpecT>> {
        if let Some(fb_c_specs) = self.build_index().component_specs()
            && self.cached_component_specs.load().is_empty()
        {
            let build_options = self.option_values();
            let component_specs =
                fb_component_specs_to_component_specs(&fb_c_specs, &build_options);

            self.cached_component_specs.store(Arc::new(component_specs));
        }

        Cow::Owned((**self.cached_component_specs.load()).clone())
    }
}

impl Package for IndexedPackage {
    // Only used for embedded_as_packages() method, which can't easily
    // return a IndexedPackage for an embedded package without
    // generating a flatbuffer bytes representation of each decoded
    // EmbeddedPackageSpec pieces. So this uses a full package spec
    // for the return values of that method.
    type Package = crate::v0::PackageSpec;
    type EmbeddedPackage = EmbeddedPackageSpec;

    #[inline]
    fn ident(&self) -> &BuildIdent {
        &self.build_ident
    }

    fn metadata(&self) -> &crate::metadata::Meta {
        let err =
            Error::SpkIndexedPackageDoesNotImplement("Package".to_string(), "metadata".to_string());
        // TODO: should this change the return value, update all the
        // caller's handling, and return an error for this implementation?
        unreachable!("{err}");
    }

    fn matches_all_filters(&self, filter_by: &Option<Vec<OptFilter>>) -> bool {
        // A copy of the code in V0 Spec's matchers_all_filters
        if let Some(filters) = filter_by {
            let settings = self.option_values();

            for filter in filters {
                if !settings.contains_key(&filter.name) {
                    // Not having an option with the filter's name is
                    // considered a match.
                    continue;
                }

                let var_request =
                    VarRequest::new_with_value(filter.name.clone(), filter.value.clone());

                let compat = self.check_satisfies_request(&var_request);
                if !compat.is_ok() {
                    return false;
                }
            }
        }
        // All the filters match, or there were no filters
        true
    }

    fn sources(&self) -> &Vec<SourceSpec> {
        let err =
            Error::SpkIndexedPackageDoesNotImplement("Package".to_string(), "sources".to_string());
        // TODO: should this change the return value, update all the
        // caller's handling, and return an error for this implementation?
        unreachable!("{err}");
    }

    fn embedded(&self) -> Cow<'_, EmbeddedPackagesList<EmbeddedPackageSpec>> {
        if let Some(fb_embedded) = self.build_index().embedded()
            && self.cached_embedded.load().is_default()
        {
            let embedded_package_specs =
                fb_embedded_package_specs_to_embedded_package_specs(&fb_embedded);

            self.cached_embedded.store(Arc::new(embedded_package_specs));
        }

        Cow::Owned((**self.cached_embedded.load()).clone())
    }

    fn embedded_as_packages(
        &self,
    ) -> std::result::Result<Vec<(Self::Package, Option<Component>)>, &str> {
        Ok(self
            .embedded()
            .iter()
            .map(|embed| (embed.clone().into(), None))
            .collect())
    }

    fn get_build_options(&self) -> Cow<'_, [Opt]> {
        if self.build_index().build_options().is_some() && self.cached_build_opts.load().is_empty()
        {
            let build_options = fb_opts_to_opts(self.build_index().build_options());
            self.cached_build_opts.store(Arc::new(build_options))
        }

        Cow::Owned((**self.cached_build_opts.load()).clone())
    }

    fn get_build_requirements(&self) -> crate::Result<Cow<'_, RequirementsList<PinnedRequest>>> {
        Err(Error::SpkIndexedPackageDoesNotImplement(
            "Package".to_string(),
            "get_build_requirements".to_string(),
        ))
    }

    fn runtime_requirements(&self) -> Cow<'_, RequirementsList<RequestWithOptions>> {
        if self.build_index().runtime_requirements().is_some()
            && self.cached_requirements.load().is_default()
        {
            let reqs = fb_requirements_to_requirements(self.build_index().runtime_requirements());
            let requirements = unsafe { RequirementsList::<RequestWithOptions>::new_checked(reqs) };

            self.cached_requirements.store(Arc::new(requirements));
        }

        Cow::Owned((**self.cached_requirements.load()).clone())
    }

    fn get_all_tests(&self) -> Vec<SpecTest> {
        let err = Error::SpkIndexedPackageDoesNotImplement(
            "Package".to_string(),
            "get_all_tests".to_string(),
        );
        // TODO: should this change the return value, update all the
        // caller's handling, and return an error for this implementation?
        unreachable!("{err}");
    }
}

impl DownstreamRequirements for IndexedPackage {
    fn downstream_build_requirements<'a>(
        &self,
        _components: impl IntoIterator<Item = &'a Component>,
    ) -> Cow<'_, RequirementsList<RequestWithOptions>> {
        // This is for build var requirements and inheritance used in
        // building. This kinds of package has no build data stored.
        let err = Error::SpkIndexedPackageDoesNotImplement(
            "DownstreamRequirements".to_string(),
            "downstream_build_requirements".to_string(),
        );
        // TODO: should this change the return value, update all the
        // caller's handling, and return an error for this implementation?
        unreachable!("{err}");
    }

    fn downstream_runtime_requirements<'a>(
        &self,
        _components: impl IntoIterator<Item = &'a Component>,
    ) -> Cow<'_, RequirementsList<RequestWithOptions>> {
        // This is also for build var requirements and inheritance
        // used in building. This package has no build data stored.
        let err = Error::SpkIndexedPackageDoesNotImplement(
            "DownstreamRequirements".to_string(),
            "downstream_runtime_requirements".to_string(),
        );
        // TODO: should this change the return value, update all the
        // caller's handling, and return an error for this implementation?
        unreachable!("{err}");
    }
}

impl OptionValues for IndexedPackage {
    fn option_values(&self) -> OptionMap {
        let mut option_map = OptionMap::default();

        for opt in fb_opts_to_opts(self.build_index().build_options()).iter() {
            let value = opt.get_value(None);
            let name = opt.full_name();
            option_map.insert(name.into(), value.to_string());
        }

        option_map
    }
}

impl PackageMut for IndexedPackage {
    fn set_build(&mut self, _build: Build) {
        let err = Error::SpkIndexedPackageDoesNotImplement(
            "PackageMut".to_string(),
            "set_build".to_string(),
        );
        // TODO: should this change the return value, update all the
        // caller's handling, and return an error for this implementation?
        unreachable!("{err}");
    }

    fn insert_or_merge_install_requirement(&mut self, _req: PinnedRequest) -> Result<()> {
        Err(Error::SpkIndexedPackageDoesNotImplement(
            "PackageMut".to_string(),
            "insert_or_merge_install_requirement".to_string(),
        ))
    }
}
