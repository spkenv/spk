// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

#![deny(unsafe_op_in_unsafe_fn)]
#![warn(clippy::fn_params_excessive_bools)]

pub use spk_schema::recipe;
pub use {serde_json, spfs};

/// Creates a repository containing a set of provided package specs.
/// It will take care of publishing the spec, and creating a build for
/// each provided package so that it can be resolved.
///
/// make_repo!({"pkg": "mypkg/1.0.0"});
/// make_repo!({"pkg": "mypkg/1.0.0"}, options = {"debug" => "off"});
#[macro_export]
macro_rules! make_repo {
    ( [ $( $spec:tt ),+ $(,)? ] ) => {{
        make_repo!([ $( $spec ),* ], options={})
    }};
    ( [ $( $spec:tt ),+ $(,)? ], options={ $($k:expr => $v:expr),* } ) => {{
        let options = spk_schema::foundation::option_map!{$($k => $v),*};
        make_repo!([ $( $spec ),* ], options=options)
    }};
    ( [ $( $spec:tt ),+ $(,)? ], options=$options:expr ) => {{
        tracing::debug!("creating in-memory repository");
        let repo = spk_storage::RepositoryHandle::new_mem();
        let _opts = $options;
        $(
            let (s, cmpts) = $crate::make_package!(repo, $spec, &_opts);
            tracing::trace!(pkg=%spk_schema::Package::ident(&s), cmpts=?cmpts.keys(), "adding package to repo");
            repo.publish_package(&s, &cmpts).await.unwrap();
        )*
        repo
    }};
}

#[macro_export(local_inner_macros)]
macro_rules! make_package {
    ($repo:ident, ($build_spec:expr, $components:expr), $opts:expr) => {{
        ($build_spec, $components)
    }};
    ($repo:ident, $build_spec:ident, $opts:expr) => {{
        use spk_schema::Package;
        let s = $build_spec.clone();
        let cmpts: std::collections::HashMap<_, $crate::spfs::encoding::Digest> = s
            .components()
            .iter()
            .map(|c| (c.name.clone(), $crate::spfs::encoding::EMPTY_DIGEST.into()))
            .collect();
        (s, cmpts)
    }};
    ($repo:ident, $spec:tt, $opts:expr) => {{
        let json = $crate::serde_json::json!($spec);
        let spec: spk_schema::v0::Spec<spk_schema::ident::AnyIdent> =
            $crate::serde_json::from_value(json).expect("Invalid spec json");
        match spec.pkg.build().map(|b| b.clone()) {
            None => {
                let recipe = spec.clone().map_ident(|i| i.into_base()).into();
                $repo.force_publish_recipe(&recipe).await.unwrap();
                make_build_and_components!(recipe, [], $opts, [])
            }
            Some(spk_schema::foundation::ident_build::Build::Source) => {
                let recipe = spec.clone().map_ident(|i| i.into_base());
                $repo.force_publish_recipe(&recipe.into()).await.unwrap();
                let build = spec.map_ident(|i| {
                    i.into_base()
                        .into_build(spk_schema::foundation::ident_build::Build::Source)
                });
                make_build_and_components!(build, [], $opts, [])
            }
            Some(b) => {
                let build = spec.map_ident(|i| i.into_base().into_build(b));
                make_build_and_components!(build, [], $opts, [])
            }
        }
    }};
}

/// Make a build of a package spec
///
/// This macro at least takes a spec json or identifier, but can optionally
/// take two additional parameters:
///     a list of dependencies used to make the build (eg [depa, depb])
///     the options used to make the build (eg: {"debug" => "on"})
#[macro_export(local_inner_macros)]
macro_rules! make_build {
    ($spec:tt) => {
        make_build!($spec, [])
    };
    ($spec:tt, $deps:tt) => {
        make_build!($spec, $deps, {})
    };
    ($spec:tt, $deps:tt, $opts:tt) => {{
        let (spec, _) = make_build_and_components!($spec, $deps, $opts);
        spec
    }};
}

/// Given a spec and optional params, creates a publishable build and component map.
///
/// This macro at least takes a spec json or identifier, but can optionally
/// take three additional parameters:
///     a list of dependencies used to make the build (eg [depa, depb])
///     the options used to make the build (eg: {"debug" => "on"})
///     the list of component names to generate (eg: ["bin", "run"])
#[macro_export(local_inner_macros)]
macro_rules! make_build_and_components {
    ($spec:tt) => {
        make_build_and_components!($spec, [])
    };
    ($spec:tt, [$($dep:expr),*]) => {
        make_build_and_components!($spec, [$($dep),*], {})
    };
    ($spec:tt, [$($dep:expr),*], $opts:tt) => {
        make_build_and_components!($spec, [$($dep),*], $opts, [])
    };
    ($spec:tt, [$($dep:expr),*], { $($k:expr => $v:expr),* }, [$($component:expr),*]) => {{
        let opts = spk_schema::foundation::option_map!{$($k => $v),*};
        make_build_and_components!($spec, [$($dep),*], opts, [$($component),*])
    }};
    ($spec:tt, [$($dep:expr),*], $opts:expr, [$($component:expr),*]) => {{
        #[allow(unused_imports)]
        use spk_schema::{Package, Recipe};
        let json = $crate::serde_json::json!($spec);
        let spec: spk_schema::v0::Spec<spk_schema::ident::AnyIdent> =
            $crate::serde_json::from_value(json).expect("Invalid spec json");
        let mut components = std::collections::HashMap::<spk_schema::foundation::ident_component::Component, $crate::spfs::encoding::Digest>::new();
        match spec.pkg.build().map(|b| b.clone()) {
            None => {
                let recipe = spec.clone().map_ident(|i| i.into_base());
                let mut build_opts = $opts.clone();
                #[allow(unused_mut)]
                let mut solution = spk_solve_solution::Solution::new(build_opts.clone());
                $(
                let dep = Arc::new($dep.clone());
                solution.add(
                    spk_schema::ident::PkgRequest::from_ident(
                        $dep.ident().to_any(),
                        spk_schema::ident::RequestedBy::SpkInternalTest,
                    ),
                    Arc::clone(&dep),
                    spk_solve_solution::PackageSource::SpkInternalTest,
                );
                )*
                let mut resolved_opts = recipe.resolve_options(&build_opts).unwrap().into_iter();
                build_opts.extend(&mut resolved_opts);
                tracing::trace!(%build_opts, "generating build");
                let build = recipe.generate_binary_build(&build_opts, &solution)
                    .expect("Failed to generate build spec");
                let mut names = std::vec![$($component.to_string()),*];
                if names.is_empty() {
                    names = build.components().iter().map(|c| c.name.to_string()).collect();
                }
                for name in names {
                    let name = spk_schema::foundation::ident_component::Component::parse(name).expect("invalid component name");
                    components.insert(name, $crate::spfs::encoding::EMPTY_DIGEST.into());
                }
                (spk_schema::Spec::V0Package(build), components)
            }
            Some(b @ spk_schema::foundation::ident_build::Build::Source) => {
                let build = spec.map_ident(|i| i.into_base().into_build(b));
                components.insert(spk_schema::foundation::ident_component::Component::Source, $crate::spfs::encoding::EMPTY_DIGEST.into());
                (spk_schema::Spec::V0Package(build), components)
            }
            Some(b) => {
                let build = spec.map_ident(|i| i.into_base().into_build(b));
                let mut names = std::vec![$($component.to_string()),*];
                if names.is_empty() {
                    names = build.components().iter().map(|c| c.name.to_string()).collect();
                }
                for name in names {
                    let name = spk_schema::foundation::ident_component::Component::parse(name).expect("invalid component name");
                    components.insert(name, $crate::spfs::encoding::EMPTY_DIGEST.into());
                }
                (spk_schema::Spec::V0Package(build), components)
            }
        }
    }}
}

/// Makes a package spec either from a raw json definition
/// or by cloning a given identifier
#[macro_export(local_inner_macros)]
macro_rules! make_spec {
    ($spec:ident) => {
        $spec.clone()
    };
    ($spec:tt) => {
        $crate::recipe!($spec)
    };
}

/// Creates a request from a literal range identifier, or json structure
#[macro_export(local_inner_macros)]
macro_rules! request {
    ($req:literal) => {
        spk_schema::ident::Request::Pkg(spk_schema::ident::PkgRequest::new(
            spk_schema::ident::parse_ident_range($req).unwrap(),
            spk_schema::ident::RequestedBy::SpkInternalTest,
        ))
    };
    ($req:ident) => {
        spk_schema::ident::Request::Pkg(spk_schema::ident::PkgRequest::new(
            spk_schema::ident::parse_ident_range($req).unwrap(),
            spk_schema::ident::RequestedBy::SpkInternalTest,
        ))
    };
    ($req:tt) => {{
        let value = serde_json::json!($req);
        let req: spk_schema::ident::Request = serde_json::from_value(value).unwrap();
        req
    }};
}
