// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

// These macros are included under the test-macros feature to keep
// them out of the main spk library. For some of them this might be a
// temporary situation. The ones that are useful in future spk
// commands may be turned into helper methods, or functions, or macros
// for using in-memory repositories at some point.

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
        let options = $crate::option_map!{$($k => $v),*};
        make_repo!([ $( $spec ),* ], options=options)
    }};
    ( [ $( $spec:tt ),+ $(,)? ], options=$options:expr ) => {{
        let repo = $crate::storage::RepositoryHandle::new_mem();
        let _opts = $options;
        $(
            let (s, cmpts) = make_package!(repo, $spec, &_opts);
            repo.publish_package(s, cmpts).unwrap();
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
        let s = $build_spec.clone();
        let cmpts: std::collections::HashMap<_, spfs::encoding::Digest> = s
            .install
            .components
            .iter()
            .map(|c| (c.name.clone(), spfs::encoding::EMPTY_DIGEST.into()))
            .collect();
        (s, cmpts)
    }};
    ($repo:ident, $spec:tt, $opts:expr) => {{
        let json = serde_json::json!($spec);
        let mut spec: $crate::api::Spec = serde_json::from_value(json).expect("Invalid spec json");
        let build = spec.clone();
        spec.pkg.set_build(None);
        $repo.force_publish_spec(spec).unwrap();
        make_build_and_components!(build, [], $opts, [])
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
        let opts = $crate::option_map!{$($k => $v),*};
        make_build_and_components!($spec, [$($dep),*], opts, [$($component),*])
    }};
    ($spec:tt, [$($dep:expr),*], $opts:expr, [$($component:expr),*]) => {{
        let mut spec = make_spec!($spec);
        let mut components = std::collections::HashMap::<$crate::api::Component, spfs::encoding::Digest>::new();
        let deps: Vec<&$crate::api::Spec> = std::vec![$(&$dep),*];
        if spec.pkg.is_source() {
            components.insert($crate::api::Component::Source, spfs::encoding::EMPTY_DIGEST.into());
            (spec, components)
        } else {
            let mut build_opts = $opts.clone();
            let mut resolved_opts = spec.resolve_all_options(&build_opts).into_iter();
            build_opts.extend(&mut resolved_opts);
            spec.update_for_build(&build_opts, deps)
                .expect("Failed to render build spec");
            let mut names = std::vec![$($component.to_string()),*];
            if names.is_empty() {
                names = spec.install.components.iter().map(|c| c.name.to_string()).collect();
            }
            for name in names {
                let name = $crate::api::Component::parse(name).expect("invalid component name");
                components.insert(name, spfs::encoding::EMPTY_DIGEST.into());
            }
            (spec, components)
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
        $crate::spec!($spec)
    };
}
