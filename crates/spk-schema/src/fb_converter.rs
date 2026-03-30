// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::collections::{BTreeMap, BTreeSet};
use std::str::FromStr;
use std::sync::Arc;

use spk_schema_foundation::IsDefault;
use spk_schema_foundation::ident::{
    BuildIdent,
    InclusionPolicy,
    OptVersionIdent,
    PinPolicy,
    PkgRequest,
    PkgRequestOptionValue,
    PkgRequestOptions,
    PkgRequestWithOptions,
    PreReleasePolicy,
    RangeIdent,
    RequestWithOptions,
};
use spk_schema_foundation::ident_build::{Build, EmbeddedSource, EmbeddedSourcePackage};
use spk_schema_foundation::ident_component::{Component, ComponentBTreeSetBuf};
use spk_schema_foundation::ident_ops::parsing::NormalizedVersionString;
use spk_schema_foundation::name::{OptNameBuf, PkgNameBuf, RepositoryNameBuf};
use spk_schema_foundation::option_map::OptionMap;
use spk_schema_foundation::spec_ops::FileMatcher;
use spk_schema_foundation::version_range::VersionFilter;

use crate::component_embedded_packages::ComponentEmbeddedPackage;
use crate::deprecate::Deprecate;
use crate::ident_build::BuildId;
use crate::ident_ops::parsing::IdentPartsBuf;
use crate::option::{PkgOpt, VarOpt};
use crate::package::{BuildOptions, Components};
use crate::v0::{EmbeddedBuildSpec, EmbeddedInstallSpec, EmbeddedPackageSpec};
use crate::version::{Compat, CompatRule, Epsilon, TagSet, Version, VersionParts};
use crate::{
    ComponentEmbeddedPackagesList,
    ComponentSpec,
    ComponentSpecList,
    EmbeddedPackagesList,
    Inheritance,
    Opt,
    RequirementsList,
};

/// Helper for turning a Vec into to flatbuffer Some(list) or None, if
/// the list is empty.
#[macro_export]
macro_rules! flatbuffer_vector {
    ($builder:ident, $list:expr) => {
        if $list.is_empty() {
            None
        } else {
            Some($builder.create_vector(&$list))
        }
    };
}

// Flatbuffer conversions functions. Kept together for now. In future,
// they may be split up across objects and traits.

#[inline]
pub fn fb_prerelease_policy_to_prerelease_policy(
    fb_prerelease_policy: spk_proto::PreReleasePolicy,
) -> Option<PreReleasePolicy> {
    match fb_prerelease_policy {
        spk_proto::PreReleasePolicy::None => None,
        spk_proto::PreReleasePolicy::ExcludeAll => Some(PreReleasePolicy::ExcludeAll),
        spk_proto::PreReleasePolicy::IncludeAll => Some(PreReleasePolicy::IncludeAll),
        _ => {
            // Covering up to ::MAX for the compiler, but this should not happen
            debug_assert!(
                false,
                "Unhandled spk_proto::PreReleasePolicy enum number encountered"
            );
            None
        }
    }
}

#[inline]
pub fn fb_inclusion_policy_to_inclusion_policy(
    fb_inclusion_policy: spk_proto::InclusionPolicy,
) -> InclusionPolicy {
    match fb_inclusion_policy {
        spk_proto::InclusionPolicy::Always => InclusionPolicy::Always,
        spk_proto::InclusionPolicy::IfAlreadyPresent => InclusionPolicy::IfAlreadyPresent,
        _ => {
            // Covering up to ::MAX for the compiler, but this should not happen
            debug_assert!(
                false,
                "Unhandled spk_proto::InclusionPolicy enum number encountered"
            );
            InclusionPolicy::default()
        }
    }
}

#[inline]
pub fn fb_lone_compat_rule_to_lone_compat_rule(
    fb_compat_rule: spk_proto::LoneCompatRule,
) -> Option<CompatRule> {
    match fb_compat_rule {
        spk_proto::LoneCompatRule::API => Some(CompatRule::API),
        spk_proto::LoneCompatRule::Binary => Some(CompatRule::Binary),
        // Includes spk_proto::LoneCompatRule::None, which means no
        // CompatRule was present.
        _ => None,
    }
}

#[inline]
pub fn fb_component_to_component(fb_component: &spk_proto::Component) -> Component {
    match fb_component.kind() {
        spk_proto::ComponentEnum::All => Component::All,
        spk_proto::ComponentEnum::Build => Component::Build,
        spk_proto::ComponentEnum::Run => Component::Run,
        spk_proto::ComponentEnum::Source => Component::Source,
        spk_proto::ComponentEnum::Named => {
            // Named enum variant means there will be a value in the
            // component's name field.
            Component::Named(
                fb_component
                    .name()
                    .expect("A ComponentEnum::Named in flatbuffer data should not have None in its name field")
                    .to_string(),
            )
        }
        _ => {
            // Covering up to ::MAX for the compiler, but this should not happen
            debug_assert!(
                false,
                "Unhandled spk_proto::ComponentEnum enum number encountered"
            );
            Component::Run
        }
    }
}

#[inline]
pub fn fb_component_names_to_component_names(
    fb_component_names: &flatbuffers::Vector<
        '_,
        flatbuffers::ForwardsUOffset<spk_proto::Component<'_>>,
    >,
) -> Vec<Component> {
    fb_component_names
        .iter()
        .map(|c| fb_component_to_component(&c))
        .collect()
}

#[inline]
pub fn fb_component_names_to_component_names_set(
    fb_component_names: &Option<
        flatbuffers::Vector<'_, flatbuffers::ForwardsUOffset<spk_proto::Component<'_>>>,
    >,
) -> BTreeSet<Component> {
    if let Some(cs) = fb_component_names {
        cs.iter().map(|c| fb_component_to_component(&c)).collect()
    } else {
        Default::default()
    }
}

#[inline]
pub fn fb_component_specs_to_component_names(
    fb_components: &flatbuffers::Vector<
        '_,
        flatbuffers::ForwardsUOffset<spk_proto::SolverComponentSpec<'_>>,
    >,
) -> Vec<Component> {
    fb_components
        .iter()
        .map(|c| fb_component_to_component(&c.name()))
        .collect()
}

#[inline]
pub fn fb_component_specs_to_component_name_set(
    fb_components: &flatbuffers::Vector<
        '_,
        flatbuffers::ForwardsUOffset<spk_proto::SolverComponentSpec<'_>>,
    >,
) -> BTreeSet<Component> {
    fb_components
        .iter()
        .map(|c| fb_component_to_component(&c.name()))
        .collect()
}

pub fn fb_requirements_to_requirements(
    requirements_with_options: Option<
        flatbuffers::Vector<
            '_,
            flatbuffers::ForwardsUOffset<spk_proto::RequirementWithOptions<'_>>,
        >,
    >,
) -> Vec<RequestWithOptions> {
    let mut requirements = Vec::new();

    if let Some(fb_reqs) = requirements_with_options {
        for fb_req in fb_reqs.iter() {
            match fb_req.request_type() {
                spk_proto::RequestWithOptions::VarRequestPinnedValue => {
                    if let Some(fb_var_req) = fb_req.request_as_var_request_pinned_value()
                        && let Some(value) = fb_var_req.value()
                    {
                        let name =
                            unsafe { OptNameBuf::from_string(fb_var_req.name().to_string()) };
                        let var_req =
                            spk_schema_foundation::ident::VarRequest::new_with_value(name, value);

                        requirements.push(RequestWithOptions::Var(var_req))
                    }
                }
                spk_proto::RequestWithOptions::PkgRequestWithOptions => {
                    if let Some(fb_pkg_req) = fb_req.request_as_pkg_request_with_options() {
                        let repo_name = if let Some(n) = fb_pkg_req.repo_name() {
                            let name = unsafe { RepositoryNameBuf::from_string(n.to_string()) };
                            Some(name)
                        } else {
                            None
                        };

                        let ident_name =
                            unsafe { PkgNameBuf::from_string(fb_pkg_req.name().to_string()) };

                        let components =
                            fb_component_names_to_component_names_set(&fb_pkg_req.components());

                        // TODO: this will be the next thing to get a
                        // proper flatbuffer representation because it
                        // is now showing up as the next significant
                        // chunk on the large solve flamegraphs
                        let version_filter =
                            fb_version_filter_to_version_filter(fb_pkg_req.version_filter());

                        let build = get_build_from_fb_pkg_request_with_options(&fb_pkg_req);

                        let prerelease_policy = fb_prerelease_policy_to_prerelease_policy(
                            fb_pkg_req.prerelease_policy(),
                        );
                        let inclusion_policy =
                            fb_inclusion_policy_to_inclusion_policy(fb_pkg_req.inclusion_policy());

                        let pin = fb_pin_to_pin(&fb_pkg_req.pin());
                        let pin_policy = fb_pin_policy_to_pin_policy(fb_pkg_req.pin_policy());

                        let required_compat =
                            fb_lone_compat_rule_to_lone_compat_rule(fb_pkg_req.required_compat());

                        let range_ident = RangeIdent {
                            repository_name: repo_name,
                            name: ident_name,
                            components,
                            version: version_filter,
                            build,
                        };

                        let pkg_request = PkgRequest {
                            pkg: range_ident,
                            prerelease_policy,
                            inclusion_policy,
                            pin,
                            pin_policy,
                            required_compat,
                            requested_by: Default::default(),
                        };

                        let options = fb_pkg_request_option_values_to_pkg_request_options(
                            fb_pkg_req.options(),
                        );

                        let pkg_req = PkgRequestWithOptions {
                            pkg_request,
                            options,
                        };

                        requirements.push(RequestWithOptions::Pkg(pkg_req));
                    }
                }
                _ => {
                    // Covering up to ::MAX for the compiler, but this should not happen
                    debug_assert!(
                        false,
                        "Unhandled spk_proto::RequestWithOptions enum number encountered"
                    );
                }
            };
        }
    }

    requirements
}

pub fn fb_component_specs_to_component_specs(
    fb_c_specs: &flatbuffers::Vector<
        '_,
        flatbuffers::ForwardsUOffset<spk_proto::SolverComponentSpec<'_>>,
    >,
    build_options: &OptionMap,
) -> ComponentSpecList<ComponentSpec> {
    let mut component_specs = Vec::new();

    for c_spec in fb_c_specs.iter() {
        let fb_component = c_spec.name();
        let component_name = fb_component_to_component(&fb_component);

        let uses = if let Some(fb_uses) = c_spec.uses() {
            fb_component_names_to_component_names(&fb_uses)
        } else {
            Vec::new()
        };

        let component_requirements =
            fb_requirements_to_requirements(c_spec.requirements_with_options());
        let component_reqs_list =
            unsafe { RequirementsList::<RequestWithOptions>::new_checked(component_requirements) }
                .into();

        let embedded = fb_component_emb_pkgs_to_component_emb_pkgs(c_spec.embedded_components());
        let component_embedded_packages: ComponentEmbeddedPackagesList =
            embedded.into_iter().into();

        // This value of this doesn't matter for solving but
        // helps avoid false mismatches in tests
        let file_matcher = if component_name == Component::Build || component_name == Component::Run
        {
            // A single '*' rule
            FileMatcher::all()
        } else {
            // Empty of rules
            FileMatcher::default()
        };

        let component_spec = unsafe {
            ComponentSpec::new_unchecked(
                component_name,
                uses,
                file_matcher,
                build_options,
                component_reqs_list,
                component_embedded_packages,
            )
        };

        component_specs.push(component_spec);
    }

    ComponentSpecList::<ComponentSpec>::new(component_specs)
}

pub fn fb_embedded_package_specs_to_embedded_package_specs(
    fb_embedded: &flatbuffers::Vector<
        '_,
        flatbuffers::ForwardsUOffset<spk_proto::SolverEmbeddedPackageSpec<'_>>,
    >,
) -> EmbeddedPackagesList<EmbeddedPackageSpec> {
    let mut embedded_package_specs = EmbeddedPackagesList::default();

    for fb_emb_spec in fb_embedded.iter() {
        let build_ident = BuildIdent::from_str(fb_emb_spec.ident()).unwrap_or_else(|_| unreachable!("An Embedded package spec in flatbuffer data should have a valid ident: '{}' is not valid", fb_emb_spec.ident()));

        let build_options = fb_opts_to_opts(fb_emb_spec.build_options());
        let options: OptionMap = build_options
            .iter()
            .map(|opt| (opt.full_name().to_owned(), opt.get_value(None)))
            .collect();

        let component_specs = if let Some(fb_c_specs) = fb_emb_spec.component_specs() {
            fb_component_specs_to_component_specs(&fb_c_specs, &options)
        } else {
            Default::default()
        };

        let requirements = fb_requirements_to_requirements(fb_emb_spec.requirements());

        let embedded_spec = unsafe {
            EmbeddedPackageSpec::new_unchecked(
                build_ident,
                EmbeddedBuildSpec {
                    options: build_options,
                },
                RequirementsList::<RequestWithOptions>::new_checked(requirements),
                EmbeddedInstallSpec {
                    // TODO: does this need to be kept, they're the old data format??
                    requirements: RequirementsList::default(),
                    components: component_specs,
                },
            )
        };

        embedded_package_specs.push(embedded_spec)
    }
    embedded_package_specs
}

// Note: fb_compat objects in packages are not optional in the rust
// structs, but fb_compat's stored in var opts are optional in the
// rust struct.
#[inline]
fn var_opt_fb_compat_to_var_opt_compat(fb_compat: Option<&str>) -> Option<Compat> {
    // None stored as an fb_compat represents no compat specified at
    // all for a var opt. Some compat stored means a compat was
    // specified, and it may even be the same as the default compat.
    fb_compat.map(|fc| unsafe {
        Compat::new_unchecked(fc)
            .expect("A Compat in flatbuffer data should be a valid Compat when parsed")
    })
}

#[inline]
pub fn fb_compat_to_compat(fb_compat: Option<&str>) -> Compat {
    if let Some(compat) = fb_compat {
        unsafe { Compat::new_unchecked(compat) }
            .expect("A Compat in flatbuffer data should be a valid Compat when parsed")
    } else {
        // In this case, None, so nothing, stored as an fb_compat
        // represents the default compat. This is different to the var
        // opt compat method above.
        Compat::default()
    }
}

#[inline]
pub fn fb_pin_to_pin(pin: &Option<&str>) -> Option<String> {
    pin.as_ref().map(|p| p.to_string())
}

#[inline]
pub fn fb_pin_policy_to_pin_policy(pin_policy: spk_proto::PinPolicy) -> PinPolicy {
    match pin_policy {
        spk_proto::PinPolicy::Required => PinPolicy::Required,
        spk_proto::PinPolicy::IfPresentInBuildEnv => PinPolicy::IfPresentInBuildEnv,
        _ => {
            // Covering up to ::MAX for the compiler, but this should not happen
            debug_assert!(
                false,
                "Unhandled spk_proto::PinPolicy enum number encountered"
            );
            PinPolicy::Required
        }
    }
}

#[inline]
fn fb_value_to_value(fb_value: Option<&str>) -> Option<String> {
    fb_value.map(|v| v.to_string())
}

pub fn fb_pkg_opt_to_opt(fb_pkg_opt: spk_proto::PkgOpt) -> Opt {
    let name = unsafe { PkgNameBuf::from_string(fb_pkg_opt.name().to_string()) };

    let components = if let Some(fb_components) = fb_pkg_opt.components() {
        ComponentBTreeSetBuf::from(fb_components.iter().map(|c| fb_component_to_component(&c)))
    } else {
        Default::default()
    };

    let prerelease_policy =
        fb_prerelease_policy_to_prerelease_policy(fb_pkg_opt.prerelease_policy());

    let required_compat = fb_lone_compat_rule_to_lone_compat_rule(fb_pkg_opt.required_compat());

    let value = fb_value_to_value(fb_pkg_opt.value());

    let po = unsafe {
        PkgOpt::new_unchecked(name, components, prerelease_policy, value, required_compat)
    };

    Opt::Pkg(po)
}

pub fn fb_var_opt_to_opt(fb_var_opt: spk_proto::VarOpt) -> Opt {
    let inheritance = match fb_var_opt.inheritance() {
        spk_proto::Inheritance::Weak => Inheritance::Weak,
        spk_proto::Inheritance::StrongForBuildOnly => Inheritance::StrongForBuildOnly,
        spk_proto::Inheritance::Strong => Inheritance::Strong,
        _ => {
            // Covering up to ::MAX for the compiler, but this should not happen
            debug_assert!(
                false,
                "Unhandled spk_proto::Inheritance enum number encountered"
            );
            Inheritance::default()
        }
    };

    let compat = var_opt_fb_compat_to_var_opt_compat(fb_var_opt.compat());

    let required = fb_var_opt.required();

    let value = fb_value_to_value(fb_var_opt.value());

    let vo =
        unsafe { VarOpt::new_unchecked(fb_var_opt.name(), inheritance, compat, required, value) };

    Opt::Var(vo)
}

#[inline]
pub fn fb_opt_to_opt(fb_opt: &spk_proto::Opt<'_>) -> Opt {
    match fb_opt.opt_type() {
        spk_proto::OptEnum::PkgOpt => {
            if let Some(fb_pkg_opt) = fb_opt.opt_as_pkg_opt() {
                fb_pkg_opt_to_opt(fb_pkg_opt)
            } else {
                unreachable!(
                    "The Pkg Opt flatbuffer data should not be None when OptEnum::PkgOpt is set"
                );
            }
        }
        spk_proto::OptEnum::VarOpt => {
            if let Some(fb_var_opt) = fb_opt.opt_as_var_opt() {
                fb_var_opt_to_opt(fb_var_opt)
            } else {
                unreachable!(
                    "The Var Opt flatbuffer data should not be None when OptEnum::VarOpt is set"
                );
            }
        }
        _ => {
            // Covering up to ::MAX for the compiler, but this should not happen
            debug_assert!(
                false,
                "Unhandled spk_proto::OptEnum enum number encountered"
            );
            unreachable!(
                "The opt_type flatbuffer data should be either OptEnum::PkgOpt or OptEnum::VarOpt"
            );
        }
    }
}

#[inline]
pub fn fb_opts_to_opts(
    fb_opts: Option<flatbuffers::Vector<'_, flatbuffers::ForwardsUOffset<spk_proto::Opt<'_>>>>,
) -> Vec<Opt> {
    if let Some(opts) = fb_opts {
        opts.iter().map(|fb_opt| fb_opt_to_opt(&fb_opt)).collect()
    } else {
        Default::default()
    }
}

fn fb_build_as_embedded_source_to_build(build: spk_proto::EmbeddedSource) -> Build {
    let es = if let Some(esp) = build.source() {
        // This a known embedded source package
        let id = esp.ident();
        let ident = IdentPartsBuf {
            repository_name: id.repository_name().map(String::from),
            pkg_name: String::from(id.pkg_name()),
            version_str: id.version_str().map(|vs| {
                // Should be safe as the data was a normalized
                // version string before it was stored.
                unsafe { NormalizedVersionString::new_unchecked(vs.to_string()) }
            }),
            build_str: id.build_str().map(String::from),
        };

        let components = fb_component_names_to_component_names_set(&esp.components());
        let unparsed = None;

        EmbeddedSource::Package(Box::new(EmbeddedSourcePackage {
            ident,
            components,
            unparsed,
        }))
    } else {
        EmbeddedSource::Unknown
    };

    Build::Embedded(es)
}

// Almost the same as the next method, but the build result can be
// optional here
#[inline]
pub fn get_build_from_fb_pkg_request_with_options(
    pkg_req: &spk_proto::PkgRequestWithOptions,
) -> Option<Build> {
    // First, handle the case when there is no build in the request
    pkg_req.build()?;

    // and then the case where there is a build in the request
    let b = match pkg_req.build_type() {
        spk_proto::Build::Source => Build::Source,
        spk_proto::Build::EmbeddedSource => {
            let build = pkg_req.build_as_embedded_source().expect("PkgRequestWithOptions in flatbuffer data should contain an embedded source build when build_type is set to EmbeddedSource");
            fb_build_as_embedded_source_to_build(build)
        }
        spk_proto::Build::BuildId => {
            let build = pkg_req.build_as_build_id().expect(
            "PkgRequestWithOptions in flatbuffer data should contain a build id when build_type is set to BuildId",
            );
            Build::BuildId(unsafe {
                BuildId::new_unchecked(build.id().chars().collect::<Vec<_>>())
            })
        }
        _ => {
            // Covering up to ::MAX for the compiler, but this should not happen
            debug_assert!(
                false,
                "Unhandled spk_proto::BuildId enum number encountered"
            );
            Build::Source
        }
    };

    Some(b)
}

// Almost the same as the previous method, but the build is not optional
#[inline]
pub fn get_build_from_fb_build_index(build_index: spk_proto::BuildIndex) -> Build {
    // A build index will always have a build value
    match build_index.build_type() {
        spk_proto::Build::Source => Build::Source,
        spk_proto::Build::EmbeddedSource => {
            let build = build_index.build_as_embedded_source().expect("A BuildIndex in flatbuffer data should contain an embedded source build when build_type is set to EmbeddedSource");
            fb_build_as_embedded_source_to_build(build)
        }
        spk_proto::Build::BuildId => {
            let build = build_index.build_as_build_id().expect(
                "A BuildIndex in flatbuffer data should contain a build id when build_type is set to BuildId",
        );
            Build::BuildId(unsafe {
                BuildId::new_unchecked(build.id().chars().collect::<Vec<_>>())
            })
        }
        _ => {
            // Covering up to ::MAX for the compiler, but this should not happen
            debug_assert!(
                false,
                "Unhandled spk_proto::BuildId enum number encountered"
            );
            Build::Source
        }
    }
}

#[inline]
pub fn fb_version_to_version(ver: spk_proto::Version) -> Version {
    let parts = VersionParts {
        parts: match ver.parts() {
            Some(numbers) => numbers.iter().collect(),
            None => Vec::new(),
        },
        epsilon: match ver.epsilon() {
            spk_proto::Epsilon::Minus => Epsilon::Minus,
            spk_proto::Epsilon::None => Epsilon::None,
            spk_proto::Epsilon::Plus => Epsilon::Plus,
            _ => {
                // Covering up to ::MAX for the compiler, but this should not happen
                debug_assert!(
                    false,
                    "Unhandled spk_proto::Epsilon enum number encountered"
                );
                Epsilon::None
            }
        },
    };
    let mut pre = BTreeMap::new();
    if let Some(tags) = ver.pre() {
        for tag_set_item in tags {
            pre.insert(tag_set_item.name().to_string(), tag_set_item.number());
        }
    }
    let mut post = BTreeMap::new();
    if let Some(tags) = ver.post() {
        for tag_set_item in tags {
            post.insert(tag_set_item.name().to_string(), tag_set_item.number());
        }
    }

    Version {
        parts,
        pre: TagSet { tags: pre },
        post: TagSet { tags: post },
    }
}

pub fn fb_component_emb_pkgs_to_component_emb_pkgs(
    component_embedded_packages: Option<
        flatbuffers::Vector<'_, flatbuffers::ForwardsUOffset<spk_proto::ComponentEmbeddedPackage>>,
    >,
) -> Vec<ComponentEmbeddedPackage> {
    if let Some(fb_comp_embedded_pkgs) = component_embedded_packages {
        fb_comp_embedded_pkgs
            .iter()
            .map(|component_embedded_pkg| {
                // Recombine name and version from this flatbuffer object make an ident
                let name =
                    unsafe { PkgNameBuf::from_string(component_embedded_pkg.name().to_string()) };
                let version = component_embedded_pkg
                    .version()
                    .map(|v| fb_version_to_version(v));
                let ident = OptVersionIdent::new(name, version);

                let components =
                    fb_component_names_to_component_names_set(&component_embedded_pkg.components());

                unsafe { ComponentEmbeddedPackage::new_unchecked(ident, components) }
            })
            .collect()
        // TODO: This does not set fabricated, not sure if that is a problem or not?
    } else {
        Vec::new()
    }
}

pub fn fb_pkg_request_option_values_to_pkg_request_options(
    pr_options: Option<
        flatbuffers::Vector<flatbuffers::ForwardsUOffset<spk_proto::PkgRequestOptionValue>>,
    >,
) -> PkgRequestOptions {
    let mut options = PkgRequestOptions::new();

    if let Some(fb_options) = pr_options {
        for fb_opt in fb_options.iter() {
            let name = unsafe { OptNameBuf::from_string(fb_opt.name().to_string()) };

            let opt_value = if let Some(v) = fb_opt.value() {
                v.to_string()
            } else {
                // This should not happen because the option should not
                // have been saved without a value.
                "".to_string()
            };

            let value = if fb_opt.is_complete() {
                PkgRequestOptionValue::Complete(opt_value)
            } else {
                PkgRequestOptionValue::Partial(opt_value)
            };

            options.insert(name, value);
        }
    }

    options
}

// TODO: this will change if the version filter gets a proper
// flatbuffer representation to move it away from a string. This look
// like it is advisable to do in future, based on current profiling.
#[inline]
pub fn fb_version_filter_to_version_filter(version_filter: Option<&str>) -> VersionFilter {
    if let Some(filter_string) = version_filter {
        unsafe { VersionFilter::new_unchecked(filter_string) }
    } else {
        Default::default()
    }
}

#[inline]
fn component_to_fb_component<'a>(
    builder: &mut flatbuffers::FlatBufferBuilder<'a>,
    c: &Component,
) -> flatbuffers::WIPOffset<spk_proto::Component<'a>> {
    let args = match c {
        Component::All => spk_proto::ComponentArgs {
            kind: spk_proto::ComponentEnum::All,
            name: None,
        },
        Component::Build => spk_proto::ComponentArgs {
            kind: spk_proto::ComponentEnum::Build,
            name: None,
        },
        Component::Run => spk_proto::ComponentArgs {
            kind: spk_proto::ComponentEnum::Run,
            name: None,
        },
        Component::Source => spk_proto::ComponentArgs {
            kind: spk_proto::ComponentEnum::Source,
            name: None,
        },
        Component::Named(name) => {
            let fb_name = builder.create_string(name);
            spk_proto::ComponentArgs {
                kind: spk_proto::ComponentEnum::Named,
                name: Some(fb_name),
            }
        }
    };
    spk_proto::Component::create(builder, &args)
}

// TODO: change to Iterator generic 1 of 3
#[inline]
pub fn components_to_fb_components<'a>(
    builder: &mut flatbuffers::FlatBufferBuilder<'a>,
    components: &[Component],
) -> Option<
    flatbuffers::WIPOffset<
        flatbuffers::Vector<'a, flatbuffers::ForwardsUOffset<spk_proto::Component<'a>>>,
    >,
> {
    let comps: Vec<_> = components
        .iter()
        .map(|c| component_to_fb_component(builder, c))
        .collect();

    flatbuffer_vector!(builder, comps)
}

// TODO: change to Iterator generic 2 of 3
#[inline]
fn components_setbuf_to_fb_components<'a>(
    builder: &mut flatbuffers::FlatBufferBuilder<'a>,
    components: &ComponentBTreeSetBuf,
) -> Option<
    flatbuffers::WIPOffset<
        flatbuffers::Vector<'a, flatbuffers::ForwardsUOffset<spk_proto::Component<'a>>>,
    >,
> {
    let comps: Vec<_> = components
        .iter()
        .map(|c| component_to_fb_component(builder, c))
        .collect();

    flatbuffer_vector!(builder, comps)
}

// TODO: change to Iterator generic 3 of 3
#[inline]
fn components_set_to_fb_components<'a>(
    builder: &mut flatbuffers::FlatBufferBuilder<'a>,
    components: &BTreeSet<Component>,
) -> Option<
    flatbuffers::WIPOffset<
        flatbuffers::Vector<'a, flatbuffers::ForwardsUOffset<spk_proto::Component<'a>>>,
    >,
> {
    let comps: Vec<_> = components
        .iter()
        .map(|c| component_to_fb_component(builder, c))
        .collect();

    flatbuffer_vector!(builder, comps)
}

#[inline]
fn lone_compat_rule_to_fb_lone_compat_rule(
    optional_compat_rule: Option<CompatRule>,
) -> spk_proto::LoneCompatRule {
    match optional_compat_rule {
        Some(compat_rule) => match compat_rule {
            CompatRule::API => spk_proto::LoneCompatRule::API,
            CompatRule::Binary => spk_proto::LoneCompatRule::Binary,
            // CompatRule::None is not allowed in a lone compat rules
            // used inside spk, but it still needs to be recorded in
            // the index's enum.
            CompatRule::None => spk_proto::LoneCompatRule::None,
        },
        None => spk_proto::LoneCompatRule::None,
    }
}

#[inline]
fn prerelease_policy_to_fb_prerelease_policy(
    optional_prerelease_policy: Option<PreReleasePolicy>,
) -> spk_proto::PreReleasePolicy {
    match optional_prerelease_policy {
        Some(policy) => match policy {
            PreReleasePolicy::ExcludeAll => spk_proto::PreReleasePolicy::ExcludeAll,
            PreReleasePolicy::IncludeAll => spk_proto::PreReleasePolicy::IncludeAll,
        },
        None => spk_proto::PreReleasePolicy::None,
    }
}

#[inline]
fn inclusion_policy_to_fb_inclusion_policy(
    inclusion_policy: InclusionPolicy,
) -> spk_proto::InclusionPolicy {
    match inclusion_policy {
        InclusionPolicy::Always => spk_proto::InclusionPolicy::Always,
        InclusionPolicy::IfAlreadyPresent => spk_proto::InclusionPolicy::IfAlreadyPresent,
    }
}

#[inline]
fn pin_to_fb_pin<'a>(
    builder: &mut flatbuffers::FlatBufferBuilder<'a>,
    pin: &Option<String>,
) -> Option<flatbuffers::WIPOffset<&'a str>> {
    if let Some(p) = pin {
        let fb_pin = builder.create_string(p);
        Some(fb_pin)
    } else {
        None
    }
}

#[inline]
fn pin_policy_to_fb_pin_policy(pin_policy: PinPolicy) -> spk_proto::PinPolicy {
    match pin_policy {
        PinPolicy::Required => spk_proto::PinPolicy::Required,
        PinPolicy::IfPresentInBuildEnv => spk_proto::PinPolicy::IfPresentInBuildEnv,
    }
}

fn opts_to_fb_pkg_request_option_values<'a>(
    builder: &mut flatbuffers::FlatBufferBuilder<'a>,
    pr_options: &PkgRequestOptions,
) -> Option<
    flatbuffers::WIPOffset<
        flatbuffers::Vector<'a, flatbuffers::ForwardsUOffset<spk_proto::PkgRequestOptionValue<'a>>>,
    >,
> {
    let mut fb_options = Vec::new();

    for (name, value) in pr_options.iter() {
        let fb_name = builder.create_string(name);
        let fb_value = match value {
            PkgRequestOptionValue::Complete(val) => builder.create_string(val),
            PkgRequestOptionValue::Partial(val) => builder.create_string(val),
        };
        let is_complete = matches!(*value, PkgRequestOptionValue::Complete(_));

        let fb_option_value = spk_proto::PkgRequestOptionValue::create(
            builder,
            &spk_proto::PkgRequestOptionValueArgs {
                name: Some(fb_name),
                value: Some(fb_value),
                is_complete,
            },
        );

        fb_options.push(fb_option_value)
    }

    flatbuffer_vector!(builder, fb_options)
}

pub fn opts_to_fb_opts<'a>(
    builder: &mut flatbuffers::FlatBufferBuilder<'a>,
    options: &[Opt],
) -> Option<
    flatbuffers::WIPOffset<
        flatbuffers::Vector<'a, flatbuffers::ForwardsUOffset<spk_proto::Opt<'a>>>,
    >,
> {
    let mut fb_options = Vec::new();
    for opt in options {
        let n = opt.full_name();
        let v = opt.get_value(None);

        let fb_name = builder.create_string(n);
        let fb_value = builder.create_string(&v);

        let (fb_opt, fb_opt_type) = match opt {
            Opt::Pkg(pkg_opt) => {
                let fb_components =
                    components_setbuf_to_fb_components(builder, &pkg_opt.components);
                // The pkg_opt.default field is ignored and not stored
                // in the flatbuffer data.
                let fb_prerelease_policy =
                    prerelease_policy_to_fb_prerelease_policy(pkg_opt.prerelease_policy);
                let required_compat =
                    lone_compat_rule_to_fb_lone_compat_rule(pkg_opt.required_compat);

                let fb_opt = spk_proto::PkgOpt::create(
                    builder,
                    &spk_proto::PkgOptArgs {
                        name: Some(fb_name),
                        components: fb_components,
                        prerelease_policy: fb_prerelease_policy,
                        required_compat,
                        value: Some(fb_value),
                    },
                )
                .as_union_value();

                (fb_opt, spk_proto::OptEnum::PkgOpt)
            }
            Opt::Var(var_opt) => {
                let inheritance = match var_opt.inheritance() {
                    Inheritance::Weak => spk_proto::Inheritance::Weak,
                    Inheritance::StrongForBuildOnly => spk_proto::Inheritance::StrongForBuildOnly,
                    Inheritance::Strong => spk_proto::Inheritance::Strong,
                };

                let fb_compat = var_opt_compat_to_var_opt_fb_compat(builder, &var_opt.compat);
                let required = var_opt.required;

                let fb_opt = spk_proto::VarOpt::create(
                    builder,
                    &spk_proto::VarOptArgs {
                        name: Some(fb_name),
                        inheritance,
                        compat: fb_compat,
                        required,
                        value: Some(fb_value),
                    },
                )
                .as_union_value();
                (fb_opt, spk_proto::OptEnum::VarOpt)
            }
        };

        let fb_opt = spk_proto::Opt::create(
            builder,
            &spk_proto::OptArgs {
                opt: Some(fb_opt),
                opt_type: fb_opt_type,
            },
        );

        fb_options.push(fb_opt);
    }

    flatbuffer_vector!(builder, fb_options)
}

fn component_emb_pkgs_to_fb_component_emb_pkgs<'a>(
    builder: &mut flatbuffers::FlatBufferBuilder<'a>,
    component_emb_pkgs: &ComponentEmbeddedPackagesList,
) -> Option<
    flatbuffers::WIPOffset<
        flatbuffers::Vector<
            'a,
            flatbuffers::ForwardsUOffset<spk_proto::ComponentEmbeddedPackage<'a>>,
        >,
    >,
> {
    let mut comp_emb_pkgs = Vec::new();

    // A fabricated component embedded packages list should not be
    // stored in the index. It should be treated as empty.
    if !component_emb_pkgs.is_fabricated() {
        for emb_comp in component_emb_pkgs.iter() {
            let fb_name = builder.create_string(emb_comp.pkg.name());

            let fb_version = emb_comp
                .pkg
                .target()
                .as_ref()
                .map(|ver| version_to_fb_version(builder, ver));

            let fb_components = components_set_to_fb_components(builder, emb_comp.components());

            let fb_emb_comp = spk_proto::ComponentEmbeddedPackage::create(
                builder,
                &spk_proto::ComponentEmbeddedPackageArgs {
                    name: Some(fb_name),
                    version: fb_version,
                    components: fb_components,
                },
            );

            comp_emb_pkgs.push(fb_emb_comp);
        }
    }
    flatbuffer_vector!(builder, comp_emb_pkgs)
}

pub fn component_specs_to_fb_component_specs<'a>(
    builder: &mut flatbuffers::FlatBufferBuilder<'a>,
    component_specs: &ComponentSpecList<ComponentSpec>,
) -> Option<
    flatbuffers::WIPOffset<
        flatbuffers::Vector<'a, flatbuffers::ForwardsUOffset<spk_proto::SolverComponentSpec<'a>>>,
    >,
> {
    let mut fb_component_specs = Vec::new();

    for cs in component_specs.iter() {
        let fb_component_name = component_to_fb_component(builder, &cs.name);

        let fb_uses = components_to_fb_components(builder, &cs.uses);

        let fb_requirements = requirements_with_options_to_fb_requirements_with_options(
            builder,
            cs.requirements_with_options(),
        );

        let fb_comp_emb_pkgs = component_emb_pkgs_to_fb_component_emb_pkgs(builder, &cs.embedded);

        let fb_comp_spec = spk_proto::SolverComponentSpec::create(
            builder,
            &spk_proto::SolverComponentSpecArgs {
                name: Some(fb_component_name),
                uses: fb_uses,
                requirements_with_options: fb_requirements,
                embedded_components: fb_comp_emb_pkgs,
            },
        );
        fb_component_specs.push(fb_comp_spec);
    }

    flatbuffer_vector!(builder, fb_component_specs)
}

pub fn embedded_pkg_specs_to_fb_embedded_package_specs<'a>(
    builder: &mut flatbuffers::FlatBufferBuilder<'a>,
    embedded: Arc<EmbeddedPackagesList<EmbeddedPackageSpec>>,
) -> Option<
    flatbuffers::WIPOffset<
        flatbuffers::Vector<
            'a,
            flatbuffers::ForwardsUOffset<spk_proto::SolverEmbeddedPackageSpec<'a>>,
        >,
    >,
> {
    let mut fb_embedded_specs = Vec::new();

    for emb_spec in embedded.iter() {
        let fb_emb_ident = builder.create_string(&format!("{:#}", emb_spec.ident()));

        let fb_emb_compat = compat_to_fb_compat(builder, &emb_spec.compat);
        let fb_emb_build_options = opts_to_fb_opts(builder, &emb_spec.build_options());

        let fb_emb_requirements = requirements_with_options_to_fb_requirements_with_options(
            builder,
            emb_spec.install_requirements_with_options(),
        );

        let fb_emb_component_specs =
            component_specs_to_fb_component_specs(builder, &emb_spec.components());

        let fb_emb_pkg = spk_proto::SolverEmbeddedPackageSpec::create(
            builder,
            &spk_proto::SolverEmbeddedPackageSpecArgs {
                ident: Some(fb_emb_ident),
                compat: fb_emb_compat,
                deprecated: emb_spec.is_deprecated(),
                build_options: fb_emb_build_options,
                requirements: fb_emb_requirements,
                component_specs: fb_emb_component_specs,
            },
        );

        fb_embedded_specs.push(fb_emb_pkg);
    }

    flatbuffer_vector!(builder, fb_embedded_specs)
}

pub fn requirements_with_options_to_fb_requirements_with_options<'a>(
    builder: &mut flatbuffers::FlatBufferBuilder<'a>,
    requirements: &RequirementsList<RequestWithOptions>,
) -> Option<
    flatbuffers::WIPOffset<
        flatbuffers::Vector<
            'a,
            flatbuffers::ForwardsUOffset<spk_proto::RequirementWithOptions<'a>>,
        >,
    >,
> {
    let mut reqs = Vec::new();

    for req in requirements.iter() {
        match req {
            RequestWithOptions::Var(vr) => {
                let fb_name = builder.create_string(&vr.var);
                let fb_value = builder.create_string(&vr.value);
                let fb_var_req = spk_proto::VarRequestPinnedValue::create(
                    builder,
                    &spk_proto::VarRequestPinnedValueArgs {
                        name: Some(fb_name),
                        value: Some(fb_value),
                    },
                )
                .as_union_value();

                let fb_req = spk_proto::RequirementWithOptions::create(
                    builder,
                    &spk_proto::RequirementWithOptionsArgs {
                        request: Some(fb_var_req),
                        request_type: spk_proto::RequestWithOptions::VarRequestPinnedValue,
                    },
                );

                reqs.push(fb_req);
            }
            RequestWithOptions::Pkg(pr) => {
                let fb_repo_name = pr
                    .pkg
                    .repository_name
                    .as_ref()
                    .map(|name| builder.create_string(name.as_ref()));
                let fb_name = builder.create_string(pr.pkg.name());

                let fb_components = components_set_to_fb_components(builder, &pr.pkg.components);

                // TODO: for now version filters strings, but in future a proper
                // version filter will breakdown into pieces in the flatbuffer
                let fb_version_filter = if pr.pkg.version.is_empty() {
                    None
                } else {
                    // A version filter is a bunch of version ranges, e.g.
                    //   [kg/3.10
                    //   pkg/3.10.0
                    // so need to use the alternate format here for version
                    // filters to ensure extra .0's don't get baked into requests.
                    Some(builder.create_string(&format!("{:#}", pr.pkg.version)))
                };

                let (fb_build, fb_build_type) = if let Some(build) = &pr.pkg.build {
                    let (fb, fbt) = build_to_fb_build(builder, build);
                    (Some(fb), fbt)
                } else {
                    (None, spk_proto::Build::NONE)
                };

                let prerelease_policy =
                    prerelease_policy_to_fb_prerelease_policy(pr.pkg_request.prerelease_policy);

                let inclusion_policy =
                    inclusion_policy_to_fb_inclusion_policy(pr.pkg_request.inclusion_policy);

                let fb_pin = pin_to_fb_pin(builder, &pr.pkg_request.pin);

                let fb_pin_policy = pin_policy_to_fb_pin_policy(pr.pkg_request.pin_policy);

                let fb_required_compat =
                    lone_compat_rule_to_fb_lone_compat_rule(pr.pkg_request.required_compat);

                let fb_options = opts_to_fb_pkg_request_option_values(builder, &pr.options);

                let fb_pkg_req = spk_proto::PkgRequestWithOptions::create(
                    builder,
                    &spk_proto::PkgRequestWithOptionsArgs {
                        repo_name: fb_repo_name,
                        name: Some(fb_name),
                        components: fb_components,
                        version_filter: fb_version_filter,
                        build: fb_build,
                        build_type: fb_build_type,
                        prerelease_policy,
                        inclusion_policy,
                        pin: fb_pin,
                        pin_policy: fb_pin_policy,
                        required_compat: fb_required_compat,
                        options: fb_options,
                    },
                )
                .as_union_value();

                let fb_req = spk_proto::RequirementWithOptions::create(
                    builder,
                    &spk_proto::RequirementWithOptionsArgs {
                        request: Some(fb_pkg_req),
                        request_type: spk_proto::RequestWithOptions::PkgRequestWithOptions,
                    },
                );

                reqs.push(fb_req);
            }
        }
    }

    flatbuffer_vector!(builder, reqs)
}

fn version_tags_to_fb_version_tags<'a>(
    builder: &mut flatbuffers::FlatBufferBuilder<'a>,
    version_tags: &BTreeMap<String, u32>,
) -> Option<
    flatbuffers::WIPOffset<
        flatbuffers::Vector<'a, flatbuffers::ForwardsUOffset<spk_proto::TagSetItem<'a>>>,
    >,
> {
    let mut tags = Vec::new();
    for (name, number) in version_tags {
        let tag_name = builder.create_string(name);
        let fb_tag = spk_proto::TagSetItem::create(
            builder,
            &spk_proto::TagSetItemArgs {
                name: Some(tag_name),
                number: *number,
            },
        );
        tags.push(fb_tag);
    }
    flatbuffer_vector!(builder, tags)
}

#[inline]
pub fn version_to_fb_version<'a>(
    builder: &mut flatbuffers::FlatBufferBuilder<'a>,
    version: &Version,
) -> flatbuffers::WIPOffset<spk_proto::Version<'a>> {
    let fb_parts = flatbuffer_vector!(builder, version.parts.parts);

    let fb_epsilon = match version.parts.epsilon {
        Epsilon::Minus => spk_proto::Epsilon::Minus,
        Epsilon::None => spk_proto::Epsilon::None,
        Epsilon::Plus => spk_proto::Epsilon::Plus,
    };

    let fb_pre_tags = version_tags_to_fb_version_tags(builder, &version.pre.tags);
    let fb_post_tags = version_tags_to_fb_version_tags(builder, &version.post.tags);

    spk_proto::Version::create(
        builder,
        &spk_proto::VersionArgs {
            parts: fb_parts,
            epsilon: fb_epsilon,
            pre: fb_pre_tags,
            post: fb_post_tags,
        },
    )
}

pub fn build_to_fb_build<'a>(
    builder: &mut flatbuffers::FlatBufferBuilder<'a>,
    build: &Build,
) -> (
    flatbuffers::WIPOffset<flatbuffers::UnionWIPOffset>,
    spk_proto::Build,
) {
    let fb_build_id = match build {
        Build::Source => {
            spk_proto::Source::create(builder, &spk_proto::SourceArgs {}).as_union_value()
        }
        Build::Embedded(es) => {
            let source = match es {
                EmbeddedSource::Package(esp) => {
                    let fb_repository_name = esp
                        .ident
                        .repository_name
                        .as_ref()
                        .map(|s| builder.create_string(s));
                    let fb_pkg_name = builder.create_string(&esp.ident.pkg_name);
                    let fb_version_str = esp
                        .ident
                        .version_str
                        .as_ref()
                        .map(|s| builder.create_string(s.as_str()));
                    let fb_build_str = esp
                        .ident
                        .build_str
                        .as_ref()
                        .map(|s| builder.create_string(s));

                    let fb_ident_parts_buf = spk_proto::IdentPartsBuf::create(
                        builder,
                        &spk_proto::IdentPartsBufArgs {
                            repository_name: fb_repository_name,
                            pkg_name: Some(fb_pkg_name),
                            version_str: fb_version_str,
                            build_str: fb_build_str,
                        },
                    );

                    let fb_components = components_set_to_fb_components(builder, &esp.components);

                    let fb_esp = spk_proto::EmbeddedSourcePackage::create(
                        builder,
                        &spk_proto::EmbeddedSourcePackageArgs {
                            ident: Some(fb_ident_parts_buf),
                            components: fb_components,
                        },
                    );
                    Some(fb_esp)
                }

                EmbeddedSource::Unknown => None,
            };
            spk_proto::EmbeddedSource::create(builder, &spk_proto::EmbeddedSourceArgs { source })
                .as_union_value()
        }
        Build::BuildId(id) => {
            let fb_id = builder.create_string(&id.to_string());
            spk_proto::BuildId::create(builder, &spk_proto::BuildIdArgs { id: Some(fb_id) })
                .as_union_value()
        }
    };

    let fb_build_type = match build {
        Build::Source => spk_proto::Build::Source,
        Build::Embedded(_es) => spk_proto::Build::EmbeddedSource,
        Build::BuildId(_id) => spk_proto::Build::BuildId,
    };

    (fb_build_id, fb_build_type)
}

// Note fb_compat objects in packages are not optional in the rust
// structs, but fb_compat's stored in var opts are optional in the
// rust struct.
#[inline]
pub fn var_opt_compat_to_var_opt_fb_compat<'a>(
    builder: &mut flatbuffers::FlatBufferBuilder<'a>,
    var_opt_compat: &Option<Compat>,
) -> Option<flatbuffers::WIPOffset<&'a str>> {
    if let Some(compat) = var_opt_compat {
        // A non-empty compat specified for the var opt. This might
        // even be the default compat, but need to store it anyway
        // because the schema doesn't distinguish this yet.
        let compat_string = compat.to_string();
        Some(builder.create_string(&compat_string))
    } else {
        // No compat specified for the var opt
        None
    }
}

#[inline]
pub fn compat_to_fb_compat<'a>(
    builder: &mut flatbuffers::FlatBufferBuilder<'a>,
    compat: &Compat,
) -> Option<flatbuffers::WIPOffset<&'a str>> {
    // Package compat objects that are the default are treated as None
    if compat.is_default() {
        None
    } else {
        let compat_string = compat.to_string();
        Some(builder.create_string(&compat_string))
    }
}
