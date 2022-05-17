// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::sync::Arc;

use rstest::{fixture, rstest};
use spfs::encoding::EMPTY_DIGEST;

use super::Solver;
use crate::{api, io, option_map, spec, Error};

#[fixture]
fn solver() -> Solver {
    Solver::default()
}

/// Creates a repository containing a set of provided package specs.
/// It will take care of publishing the spec, and creating a build for
/// each provided package so that it can be resolved.
///
/// make_repo!({"pkg": "mypkg/1.0.0"});
/// make_repo!({"pkg": "mypkg/1.0.0"}, options = {"debug" => "off"});
macro_rules! make_repo {
    ( [ $( $spec:tt ),+ $(,)? ] ) => {{
        make_repo!([ $( $spec ),* ], options={})
    }};
    ( [ $( $spec:tt ),+ $(,)? ], options={ $($k:expr => $v:expr),* } ) => {{
        let options = crate::option_map!{$($k => $v),*};
        make_repo!([ $( $spec ),* ], options=options)
    }};
    ( [ $( $spec:tt ),+ $(,)? ], options=$options:expr ) => {{
        let repo = crate::storage::RepositoryHandle::new_mem();
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
        let mut spec: crate::api::Spec = serde_json::from_value(json).expect("Invalid spec json");
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
        let opts = crate::option_map!{$($k => $v),*};
        make_build_and_components!($spec, [$($dep),*], opts, [$($component),*])
    }};
    ($spec:tt, [$($dep:expr),*], $opts:expr, [$($component:expr),*]) => {{
        let mut spec = make_spec!($spec);
        let mut components = std::collections::HashMap::<crate::api::Component, spfs::encoding::Digest>::new();
        let deps: Vec<&api::Spec> = std::vec![$(&$dep),*];
        if spec.pkg.is_source() {
            components.insert(crate::api::Component::Source, spfs::encoding::EMPTY_DIGEST.into());
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
                let name = crate::api::Component::parse(name).expect("invalid component name");
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
        crate::spec!($spec)
    };
}

/// Creates a request from a literal range identifier, or json structure
macro_rules! request {
    ($req:literal) => {
        crate::api::Request::Pkg(crate::api::PkgRequest::new(
            crate::api::parse_ident_range($req).unwrap(),
        ))
    };
    ($req:tt) => {{
        let value = serde_json::json!($req);
        let req: crate::api::Request = serde_json::from_value(value).unwrap();
        req
    }};
}

/// Asserts that a package exists in the solution at a specific version,
/// or that the solution contains a specific set of packages by name.
///
/// Instead of a packages, version, this macro can also check the set
/// of resolved components, or the specific build of the package.
macro_rules! assert_resolved {
    ($solution:ident, $pkg:literal, $version:literal) => {
        assert_resolved!($solution, $pkg, $version, "wrong package version was resolved")
    };
    ($solution:ident, $pkg:literal, $version:literal, $message:literal) => {
        assert_resolved!($solution, $pkg, version = $version, $message)
    };
    ($solution:ident, $pkg:literal, version = $version:literal, $message:literal) => {{
        let pkg = $solution
            .get($pkg)
            .expect("expected package to be in solution");
        assert_eq!(pkg.spec.pkg.version, $version, $message);
    }};

    ($solution:ident, $pkg:literal, build = $build:expr) => {
        assert_resolved!($solution, $pkg, build = $build, "wrong package build was resolved")
    };
    ($solution:ident, $pkg:literal, build = $build:expr, $message:literal) => {{
        let pkg = $solution
            .get($pkg)
            .expect("expected package to be in solution");
        assert_eq!(pkg.spec.pkg.build, $build, $message);
    }};

    ($solution:ident, $pkg:literal, components = [$($component:literal),+ $(,)?]) => {{
        let mut resolved = std::collections::HashSet::<String>::new();
        let pkg = $solution
            .get($pkg)
            .expect("expected package to be in solution");
        match pkg.source {
            crate::solve::PackageSource::Repository{components, ..} => {
                resolved.extend(components.keys().map(ToString::to_string));
            }
            _ => panic!("expected pkg to have a repo source"),
        }
        let expected: std::collections::HashSet<_> = vec![
            $( $component.to_string() ),*
        ].into_iter().collect();
        assert_eq!(resolved, expected, "wrong set of components were resolved");
    }};

    ($solution:ident, [$($pkg:literal),+ $(,)?]) => {{
        let names: std::collections::HashSet<_> = $solution
            .items()
            .into_iter()
            .map(|s| s.spec.pkg.name.to_string())
            .collect();
        let expected: std::collections::HashSet<_> = vec![
            $( $pkg.to_string() ),*
        ].into_iter().collect();
        assert_eq!(names, expected, "wrong set of packages was resolved");
    }};


}

#[rstest]
fn test_solver_no_requests(mut solver: Solver) {
    solver.solve().unwrap();
}

#[rstest]
fn test_solver_package_with_no_spec(mut solver: Solver) {
    let repo = crate::storage::RepositoryHandle::new_mem();

    let options = option_map! {};
    let mut spec = spec!({"pkg": "my-pkg/1.0.0"});
    spec.pkg
        .set_build(Some(api::Build::Digest(options.digest())));

    // publish package without publishing spec
    let components = vec![(api::Component::Run, EMPTY_DIGEST.into())]
        .into_iter()
        .collect();
    repo.publish_package(spec, components).unwrap();

    solver.update_options(options);
    solver.add_repository(Arc::new(repo));
    solver.add_request(request!("my-pkg"));

    let res = io::run_and_print_resolve(&solver, 100);
    assert!(matches!(res, Err(Error::PackageNotFoundError(_))));
}

#[rstest]
fn test_solver_single_package_no_deps(mut solver: Solver) {
    let options = option_map! {};
    let repo = make_repo!([{"pkg": "my-pkg/1.0.0"}], options=options.clone());

    solver.update_options(options);
    solver.add_repository(Arc::new(repo));
    solver.add_request(request!("my-pkg"));

    let packages = io::run_and_print_resolve(&solver, 100).unwrap();
    assert_eq!(packages.len(), 1, "expected one resolved package");
    let resolved = packages.get("my-pkg").unwrap();
    assert_eq!(&resolved.spec.pkg.version.to_string(), "1.0.0");
    assert!(resolved.spec.pkg.build.is_some());
    assert_ne!(resolved.spec.pkg.build, Some(api::Build::Source));
}

#[rstest]
fn test_solver_single_package_simple_deps(mut solver: Solver) {
    let options = option_map! {};
    let repo = make_repo!(
        [
            {"pkg": "pkg-a/0.9.0"},
            {"pkg": "pkg-a/1.0.0"},
            {"pkg": "pkg-a/1.2.0"},
            {"pkg": "pkg-a/1.2.1"},
            {"pkg": "pkg-a/2.0.0"},
            {"pkg": "pkg-b/1.0.0", "install": {"requirements": [{"pkg": "pkg-a/2.0"}]}},
            {"pkg": "pkg-b/1.1.0", "install": {"requirements": [{"pkg": "pkg-a/1.2"}]}},
        ]
    );

    solver.update_options(options);
    solver.add_repository(Arc::new(repo));
    solver.add_request(request!("pkg-b/1.1"));

    let packages = io::run_and_print_resolve(&solver, 100).unwrap();
    assert_eq!(packages.len(), 2, "expected two resolved packages");
    assert_resolved!(packages, "pkg-a", "1.2.1");
    assert_resolved!(packages, "pkg-b", "1.1.0");
}

#[rstest]
fn test_solver_dependency_abi_compat(mut solver: Solver) {
    let options = option_map! {};
    let repo = make_repo!(
        [
            {
                "pkg": "pkg-b/1.1.0",
                "install": {"requirements": [{"pkg": "pkg-a/1.1.0"}]},
            },
            {"pkg": "pkg-a/2.1.1", "compat": "x.a.b"},
            {"pkg": "pkg-a/1.2.1", "compat": "x.a.b"},
            {"pkg": "pkg-a/1.1.1", "compat": "x.a.b"},
            {"pkg": "pkg-a/1.1.0", "compat": "x.a.b"},
            {"pkg": "pkg-a/1.0.0", "compat": "x.a.b"},
            {"pkg": "pkg-a/0.9.0", "compat": "x.a.b"},
        ]
    );

    solver.update_options(options);
    solver.add_repository(Arc::new(repo));
    solver.add_request(request!("pkg-b/1.1"));

    let packages = io::run_and_print_resolve(&solver, 100).unwrap();
    assert_eq!(packages.len(), 2, "expected two resolved packages");
    assert_resolved!(packages, "pkg-a", "1.1.1");
    assert_resolved!(packages, "pkg-b", "1.1.0");
}

#[rstest]
fn test_solver_dependency_incompatible(mut solver: Solver) {
    // test what happens when a dependency is added which is incompatible
    // with an existing request in the stack
    let repo = make_repo!(
        [
            {"pkg": "maya/2019.0.0"},
            {"pkg": "maya/2020.0.0"},
            {
                "pkg": "my-plugin/1.0.0",
                "install": {"requirements": [{"pkg": "maya/2020"}]},
            },
        ]
    );

    solver.add_repository(Arc::new(repo));
    solver.add_request(request!("my-plugin/1"));
    // this one is incompatible with requirements of my-plugin but the solver doesn't know it yet
    solver.add_request(request!("maya/2019"));

    let res = io::run_and_print_resolve(&solver, 100);
    assert!(matches!(res, Err(Error::Solve(_))));
}

#[rstest]
fn test_solver_dependency_incompatible_stepback(mut solver: Solver) {
    // test what happens when a dependency is added which is incompatible
    // with an existing request in the stack - in this case we want the solver
    // to successfully step back into an older package version with
    // better dependencies
    let repo = make_repo!(
        [
            {"pkg": "maya/2019"},
            {"pkg": "maya/2020"},
            {
                "pkg": "my-plugin/1.1.0",
                "install": {"requirements": [{"pkg": "maya/2020"}]},
            },
            {
                "pkg": "my-plugin/1.0.0",
                "install": {"requirements": [{"pkg": "maya/2019"}]},
            },
        ]
    );

    solver.add_repository(Arc::new(repo));
    solver.add_request(request!("my-plugin/1"));
    // this one is incompatible with requirements of my-plugin/1.1.0 but not my-plugin/1.0
    solver.add_request(request!("maya/2019"));

    let packages = io::run_and_print_resolve(&solver, 100).unwrap();
    assert_resolved!(packages, "my-plugin", "1.0.0");
    assert_resolved!(packages, "maya", "2019.0.0");
}

#[rstest]
fn test_solver_dependency_already_satisfied(mut solver: Solver) {
    // test what happens when a dependency is added which represents
    // a package which has already been resolved
    // - and the resolved version satisfies the request

    let repo = make_repo!(
        [
            {
                "pkg": "pkg-top/1.0.0",
                // should resolve dep_1 as 1.0.0
                "install": {
                    "requirements": [{"pkg": "dep-1/~1.0.0"}, {"pkg": "dep-2/1"}]
                },
            },
            {"pkg": "dep-1/1.1.0"},
            {"pkg": "dep-1/1.0.0"},
            // when dep_2 gets resolved, it will re-request this but it has already resolved
            {"pkg": "dep-2/1.0.0", "install": {"requirements": [{"pkg": "dep-1/1"}]}},
        ]
    );
    solver.add_repository(Arc::new(repo));
    solver.add_request(request!("pkg-top"));
    let packages = io::run_and_print_resolve(&solver, 100).unwrap();

    assert_resolved!(packages, ["pkg-top", "dep-1", "dep-2"]);
    assert_resolved!(packages, "dep-1", "1.0.0");
}

#[rstest]
fn test_solver_dependency_reopen_solvable(mut solver: Solver) {
    // test what happens when a dependency is added which represents
    // a package which has already been resolved
    // - and the resolved version does not satisfy the request
    //   - and a version exists for both (solvable)

    let repo = make_repo!(
        [
            {
                "pkg": "my-plugin/1.0.0",
                // should resolve maya as 2019.2 (favoring latest)
                "install": {
                    "requirements": [{"pkg": "maya/2019"}, {"pkg": "some-library/1"}]
                },
            },
            {"pkg": "maya/2019.2.0"},
            {"pkg": "maya/2019.0.0"},
            // when some-library gets resolved, it will enforce an older version
            // of the existing resolve, which is still valid for all requests
            {
                "pkg": "some-library/1.0.0",
                "install": {"requirements": [{"pkg": "maya/~2019.0.0"}]},
            },
        ]
    );
    solver.add_repository(Arc::new(repo));
    solver.add_request(request!("my-plugin"));
    let packages = io::run_and_print_resolve(&solver, 100).unwrap();
    assert_resolved!(packages, ["my-plugin", "some-library", "maya"]);
    assert_resolved!(packages, "maya", "2019.0.0");
}

#[rstest]
fn test_solver_dependency_reiterate(mut solver: Solver) {
    // test what happens when a package iterator must be run through twice
    // - walking back up the solve graph should reset the iterator to where it was

    let repo = make_repo!(
        [
            {
                "pkg": "my-plugin/1.0.0",
                "install": {"requirements": [{"pkg": "some-library/1"}]},
            },
            {"pkg": "maya/2019.2.0"},
            {"pkg": "maya/2019.0.0"},
            // asking for a maya version that doesn't exist will run out the iterator
            {
                "pkg": "some-library/1.0.0",
                "install": {"requirements": [{"pkg": "maya/~2018.0.0"}]},
            },
            // the second attempt at some-library will find maya 2019 properly
            {
                "pkg": "some-library/1.0.0",
                "install": {"requirements": [{"pkg": "maya/~2019.0.0"}]},
            },
        ]
    );
    solver.add_repository(Arc::new(repo));
    solver.add_request(request!("my-plugin"));
    let packages = io::run_and_print_resolve(&solver, 100).unwrap();
    assert_resolved!(packages, ["my-plugin", "some-library", "maya"]);
    assert_resolved!(packages, "maya", "2019.0.0");
}

#[rstest]
fn test_solver_dependency_reopen_unsolvable(mut solver: Solver) {
    // test what happens when a dependency is added which represents
    // a package which has already been resolved
    // - and the resolved version does not satisfy the request
    //   - and a version does not exist for both (unsolvable)

    let repo = make_repo!(
        [
            {
                "pkg": "pkg-top/1.0.0",
                // must resolve dep_1 as 1.1.0 (favoring latest)
                "install": {"requirements": [{"pkg": "dep-1/1.1"}, {"pkg": "dep-2/1"}]},
            },
            {"pkg": "dep-1/1.1.0"},
            {"pkg": "dep-1/1.0.0"},
            // when dep_2 gets resolved, it will enforce an older version
            // of the existing resolve, which is in conflict with the original
            {
                "pkg": "dep-2/1.0.0",
                "install": {"requirements": [{"pkg": "dep-1/~1.0.0"}]},
            },
        ]
    );
    solver.add_repository(Arc::new(repo));
    solver.add_request(request!("pkg-top"));
    let result = io::run_and_print_resolve(&solver, 100);
    assert!(matches!(result, Err(Error::Solve(_))));
}

#[rstest]
fn test_solver_pre_release_config(mut solver: Solver) {
    let repo = make_repo!(
        [
            {"pkg": "my-pkg/0.9.0"},
            {"pkg": "my-pkg/1.0.0-pre.0"},
            {"pkg": "my-pkg/1.0.0-pre.1"},
            {"pkg": "my-pkg/1.0.0-pre.2"},
        ]
    );
    let repo = Arc::new(repo);

    solver.add_repository(repo.clone());
    solver.add_request(request!("my-pkg"));

    let solution = io::run_and_print_resolve(&solver, 100).unwrap();
    assert_resolved!(
        solution,
        "my-pkg",
        "0.9.0",
        "should not resolve pre-release by default"
    );

    solver.reset();
    solver.add_repository(repo);
    solver.add_request(request!({"pkg": "my-pkg", "prereleasePolicy": "IncludeAll"}));

    let solution = io::run_and_print_resolve(&solver, 100).unwrap();
    assert_resolved!(solution, "my-pkg", "1.0.0-pre.2");
}

#[rstest]
fn test_solver_constraint_only(mut solver: Solver) {
    // test what happens when a dependency is marked as a constraint/optional
    // and no other request is added
    // - the constraint is noted
    // - the package does not get resolved into the final env

    let repo = make_repo!(
        [
            {
                "pkg": "vnp3/2.0.0",
                "install": {
                    "requirements": [
                        {"pkg": "python/3.7", "include": "IfAlreadyPresent"}
                    ]
                },
            }
        ]
    );
    solver.add_repository(Arc::new(repo));
    solver.add_request(request!("vnp3"));
    let solution = io::run_and_print_resolve(&solver, 100).unwrap();

    assert!(solution.get("python").is_none());
}

#[rstest]
fn test_solver_constraint_and_request(mut solver: Solver) {
    // test what happens when a dependency is marked as a constraint/optional
    // and also requested by another package
    // - the constraint is noted
    // - the constraint is merged with the request

    let repo = make_repo!(
        [
            {
                "pkg": "vnp3/2.0.0",
                "install": {
                    "requirements": [
                        {"pkg": "python/=3.7.3", "include": "IfAlreadyPresent"}
                    ]
                },
            },
            {
                "pkg": "my-tool/1.2.0",
                "install": {"requirements": [{"pkg": "vnp3"}, {"pkg": "python/3.7"}]},
            },
            {"pkg": "python/3.7.3"},
            {"pkg": "python/3.8.1"},
        ]
    );
    solver.add_repository(Arc::new(repo));
    solver.add_request(request!("my-tool"));
    let solution = io::run_and_print_resolve(&solver, 100).unwrap();

    assert_resolved!(solution, "python", "3.7.3");
}

#[rstest]
fn test_solver_option_compatibility(mut solver: Solver) {
    // test what happens when an option is given in the solver
    // - the options for each build are checked
    // - the resolved build must have used the option

    let spec = spec!(
        {
            "pkg": "vnp3/2.0.0",
            "build": {
                // favoritize 2.7, otherwise an option of python=2 doesn't actually
                // exclude python 3 from being resolved
                "options": [{"pkg": "python/~2.7"}],
                "variants": [{"python": "3.7"}, {"python": "2.7"}],
            },
        }
    );
    let py27 = make_build!({"pkg": "python/2.7.5"});
    let py37 = make_build!({"pkg": "python/3.7.3"});
    let for_py27 = make_build!(spec, [py27]);
    let for_py37 = make_build!(spec, [py37]);
    let repo = make_repo!([for_py27, for_py37]);
    repo.publish_spec(spec).unwrap();
    let repo = Arc::new(repo);

    for pyver in ["2", "2.7", "2.7.5", "3", "3.7", "3.7.3"] {
        solver.reset();
        solver.add_repository(repo.clone());
        solver.add_request(request!("vnp3"));
        solver.add_request(
            api::VarRequest {
                var: "python".to_string(),
                pin: false,
                value: pyver.to_string(),
            }
            .into(),
        );
        let solution = io::run_and_print_resolve(&solver, 100).unwrap();

        let resolved = solution.get("vnp3").unwrap();
        let opt = resolved.spec.build.options.get(0).unwrap();
        let value = opt.get_value(None);
        let expected = format!("~{}", pyver);
        assert!(
            value.starts_with(&expected),
            "{} should start with ~{}",
            value,
            pyver
        );
    }
}

#[rstest]
fn test_solver_option_injection(mut solver: Solver) {
    // test the options that are defined when a package is resolved
    // - options are namespaced and added to the environment

    let spec = spec!(
        {
            "pkg": "vnp3/2.0.0",
            "build": {
                "options": [
                    {"pkg": "python"},
                    {"var": "python.abi/cp27mu"},
                    {"var": "debug/on"},
                    {"var": "special"},
                ],
            },
        }
    );
    let pybuild = make_build!(
        {
            "pkg": "python/2.7.5",
            "build": {"options": [{"var": "abi/cp27mu"}]},
        }
    );
    let build = make_build!(spec, [pybuild]);
    let repo = make_repo!([build]);
    repo.publish_spec(spec).unwrap();

    solver.add_repository(Arc::new(repo));
    solver.add_request(request!("vnp3"));
    let solution = io::run_and_print_resolve(&solver, 100).unwrap();

    let mut opts = solution.options();
    assert_eq!(
        opts.remove(&String::from("vnp3")),
        Some("~2.0.0".to_string())
    );
    assert_eq!(
        opts.remove(&String::from("vnp3.python")),
        Some("~2.7.5".to_string())
    );
    assert_eq!(
        opts.remove(&String::from("vnp3.debug")),
        Some("on".to_string())
    );
    assert_eq!(
        opts.remove(&String::from("python.abi")),
        Some("cp27mu".to_string())
    );
    assert!(
        !opts.contains_key("vnp3.special"),
        "should not define empty values"
    );
    assert_eq!(opts.len(), 0, "expected no more options");
}

#[rstest]
fn test_solver_build_from_source(mut solver: Solver) {
    // test when no appropriate build exists but the source is available
    // - the build is skipped
    // - the source package is checked for current options
    // - a new build is created
    // - the local package is used in the resolve

    let repo = make_repo!(
        [
            {
                "pkg": "my-tool/1.2.0/src",
                "build": {"options": [{"var": "debug"}], "script": "echo BUILD"},
            },
            {
                "pkg": "my-tool/1.2.0",
                "build": {"options": [{"var": "debug"}], "script": "echo BUILD"},
            },
        ],
        options={"debug" => "off"}
    );
    let repo = Arc::new(repo);

    solver.add_repository(repo.clone());
    // the new option value should disqulify the existing build
    // but a new one should be generated for this set of options
    solver.add_request(request!({"var": "debug/on"}));
    solver.add_request(request!("my-tool"));

    let solution = io::run_and_print_resolve(&solver, 100).unwrap();

    let resolved = solution.get("my-tool").unwrap();
    assert!(
        resolved.is_source_build(),
        "Should set unbuilt spec as source: {}",
        resolved.spec.pkg
    );

    solver.reset();
    solver.add_repository(repo);
    solver.add_request(request!({"var": "debug/on"}));
    solver.add_request(request!("my-tool"));
    solver.set_binary_only(true);
    // Should fail when binary-only is specified
    let res = io::run_and_print_resolve(&solver, 100);
    assert!(matches!(res, Err(Error::Solve(_))));
}

#[rstest]
fn test_solver_build_from_source_unsolvable(mut solver: Solver) {
    // test when no appropriate build exists but the source is available
    // - if the requested pkg cannot resolve a build environment
    // - this is flagged by the solver as impossible

    let gcc48 = make_build!({"pkg": "gcc/4.8"});
    let build_with_48 = make_build!(
        {
            "pkg": "my-tool/1.2.0",
            "build": {"options": [{"pkg": "gcc"}], "script": "echo BUILD"},
        },
        [gcc48]
    );
    let repo = make_repo!(
        [
            gcc48,
            build_with_48,
            {
                "pkg": "my-tool/1.2.0/src",
                "build": {"options": [{"pkg": "gcc"}], "script": "echo BUILD"},
            },
        ],
        options={"gcc"=>"4.8"}
    );

    solver.add_repository(Arc::new(repo));
    // the new option value should disqualify the existing build
    // and there is no 6.3 that can be resolved for this request
    solver.add_request(request!({"var": "gcc/6.3"}));
    solver.add_request(request!("my-tool"));

    let res = io::run_and_print_resolve(&solver, 100);
    assert!(matches!(res, Err(Error::Solve(_))));
}

#[rstest]
fn test_solver_build_from_source_dependency(mut solver: Solver) {
    // test when no appropriate build exists but the source is available
    // - the existing build is skipped
    // - the source package is checked for current options
    // - a new build is created of the dependent
    // - the local package is used in the resolve

    let python36 = make_build!({"pkg": "python/3.6.3", "compat": "x.a.b"});
    let build_with_py36 = make_build!(
        {
            "pkg": "my-tool/1.2.0",
            "build": {"options": [{"pkg": "python"}]},
            "install": {"requirements": [{"pkg": "python/3.6.3"}]},
        },
        [python36]
    );

    let repo = make_repo!(
        [
            // the source package pins the build environment package
            {
                "pkg": "my-tool/1.2.0/src",
                "build": {"options": [{"pkg": "python"}]},
                "install": {
                    "requirements": [{"pkg": "python", "fromBuildEnv": "x.x.x"}]
                },
            },
            // one existing build exists that used python 3.6.3
            build_with_py36,
            // only python 3.7 exists, which is api compatible, but not abi
            {"pkg": "python/3.7.3", "compat": "x.a.b"},
        ]
    );

    // the new option value should disqualify the existing build
    // but a new one should be generated for this set of options
    solver.update_options(option_map! {"debug" => "on"});
    solver.add_repository(Arc::new(repo));
    solver.add_request(request!("my-tool"));

    let solution = io::run_and_print_resolve(&solver, 100).unwrap();

    assert!(
        solution.get("my-tool").unwrap().is_source_build(),
        "should want to build"
    );
}

#[rstest]
fn test_solver_deprecated_build(mut solver: Solver) {
    let deprecated = make_build!({"pkg": "my-pkg/1.0.0", "deprecated": true});
    let deprecated_build = deprecated.pkg.clone();
    let repo = make_repo!([
        {"pkg": "my-pkg/0.9.0"},
        {"pkg": "my-pkg/1.0.0"},
        deprecated,
    ]);
    let repo = Arc::new(repo);

    solver.add_repository(repo.clone());
    solver.add_request(request!("my-pkg"));

    let solution = io::run_and_print_resolve(&solver, 100).unwrap();
    assert_resolved!(
        solution,
        "my-pkg",
        "0.9.0",
        "should not resolve deprecated build by default"
    );

    solver.reset();
    solver.add_repository(repo);
    solver.add_request(api::PkgRequest::from_ident(&deprecated_build).into());

    let solution = io::run_and_print_resolve(&solver, 100).unwrap();
    assert_resolved!(
        solution,
        "my-pkg",
        "1.0.0",
        "should be able to resolve exact deprecated build"
    );
}

#[rstest]
fn test_solver_deprecated_version(mut solver: Solver) {
    let mut deprecated = make_build!({"pkg": "my-pkg/1.0.0"});
    deprecated.deprecated = true;
    let repo = make_repo!(
        [{"pkg": "my-pkg/0.9.0"}, {"pkg": "my-pkg/1.0.0", "deprecated": true}, deprecated]
    );
    let repo = Arc::new(repo);

    solver.add_repository(repo.clone());
    solver.add_request(request!("my-pkg"));

    let solution = io::run_and_print_resolve(&solver, 100).unwrap();
    assert_resolved!(
        solution,
        "my-pkg",
        "0.9.0",
        "should not resolve build when version is deprecated by default"
    );

    solver.reset();
    solver.add_repository(repo);
    solver.add_request(api::PkgRequest::new(api::RangeIdent::exact(&deprecated.pkg, [])).into());

    let solution = io::run_and_print_resolve(&solver, 100).unwrap();
    assert_resolved!(
        solution,
        "my-pkg",
        "1.0.0",
        "should be able to resolve exact build when version is deprecated"
    );
}

#[rstest]
fn test_solver_build_from_source_deprecated(mut solver: Solver) {
    // test when no appropriate build exists and the main package
    // has been deprecated, no source build should be allowed

    let repo = make_repo!(
        [
            {
                "pkg": "my-tool/1.2.0/src",
                "build": {"options": [{"var": "debug"}], "script": "echo BUILD"},
            },
            {
                "pkg": "my-tool/1.2.0",
                "build": {"options": [{"var": "debug"}], "script": "echo BUILD"},
            },
        ],
       options = {"debug" => "off"}
    );
    let mut spec = repo
        .read_spec(&api::parse_ident("my-tool/1.2.0").unwrap())
        .unwrap();
    spec.deprecated = true;
    repo.force_publish_spec(spec).unwrap();

    solver.add_repository(Arc::new(repo));
    solver.add_request(request!({"var": "debug/on"}));
    solver.add_request(request!("my-tool"));

    let res = io::run_and_print_resolve(&solver, 100);
    assert!(matches!(res, Err(Error::Solve(_))));
}

#[rstest]
fn test_solver_embedded_package_adds_request(mut solver: Solver) {
    // test when there is an embedded package
    // - the embedded package is added to the solution
    // - the embedded package is also added as a request in the resolve

    let repo = make_repo!(
        [
            {
                "pkg": "maya/2019.2",
                "build": {"script": "echo BUILD"},
                "install": {"embedded": [{"pkg": "qt/5.12.6"}]},
            },
        ]
    );

    solver.add_repository(Arc::new(repo));
    solver.add_request(request!("maya"));

    let solution = io::run_and_print_resolve(&solver, 100).unwrap();

    assert_resolved!(solution, "qt", build = Some(api::Build::Embedded));
    assert_resolved!(solution, "qt", "5.12.6");
    assert_resolved!(solution, "qt", build = Some(api::Build::Embedded));
}

#[rstest]
fn test_solver_embedded_package_solvable(mut solver: Solver) {
    // test when there is an embedded package
    // - the embedded package is added to the solution
    // - the embedded package resolves existing requests
    // - the solution includes the embedded packages

    let repo = make_repo!(
        [
            {
                "pkg": "maya/2019.2",
                "build": {"script": "echo BUILD"},
                "install": {"embedded": [{"pkg": "qt/5.12.6"}]},
            },
            {
                "pkg": "qt/5.13.0",
                "build": {"script": "echo BUILD"},
            },
        ]
    );

    solver.add_repository(Arc::new(repo));
    solver.add_request(request!("qt"));
    solver.add_request(request!("maya"));

    let solution = io::run_and_print_resolve(&solver, 100).unwrap();

    assert_resolved!(solution, "qt", "5.12.6");
    assert_resolved!(solution, "qt", build = Some(api::Build::Embedded));
}

#[rstest]
fn test_solver_embedded_package_unsolvable(mut solver: Solver) {
    // test when there is an embedded package
    // - the embedded package is added to the solution
    // - the embedded package conflicts with existing requests

    let repo = make_repo!(
        [
            {
                "pkg": "my-plugin",
                // the qt/5.13 requirement is available but conflits with maya embedded
                "install": {"requirements": [{"pkg": "maya/2019"}, {"pkg": "qt/5.13"}]},
            },
            {
                "pkg": "maya/2019.2",
                "build": {"script": "echo BUILD"},
                "install": {"embedded": [{"pkg": "qt/5.12.6"}]},
            },
            {
                "pkg": "qt/5.13.0",
                "build": {"script": "echo BUILD"},
            },
        ]
    );

    solver.add_repository(Arc::new(repo));
    solver.add_request(request!("my-plugin"));

    let res = io::run_and_print_resolve(&solver, 100);
    assert!(matches!(res, Err(Error::Solve(_))));
}

#[rstest]
fn test_solver_some_versions_conflicting_requests(mut solver: Solver) {
    // test when there is a package with some version that have a conflicting dependency
    // - the solver passes over the one with conflicting
    // - the solver logs compat info for versions with conflicts

    let repo = make_repo!(
        [
            {
                "pkg": "my-lib",
                "install": {
                    // python 2.7 requirement will conflict with the first (2.1) build of dep
                    "requirements": [{"pkg": "python/=2.7.5"}, {"pkg": "dep/2"}]
                },
            },
            {
                "pkg": "dep/2.1.0",
                "install": {"requirements": [{"pkg": "python/=3.7.3"}]},
            },
            {
                "pkg": "dep/2.0.0",
                "install": {"requirements": [{"pkg": "python/=2.7.5"}]},
            },
            {"pkg": "python/2.7.5"},
            {"pkg": "python/3.7.3"},
        ]
    );

    solver.add_repository(Arc::new(repo));
    solver.add_request(request!("my-lib"));

    let solution = io::run_and_print_resolve(&solver, 100).unwrap();

    assert_resolved!(solution, "dep", "2.0.0");
}

#[rstest]
fn test_solver_embedded_request_invalidates(mut solver: Solver) {
    // test when a package is resolved with an incompatible embedded pkg
    // - the solver tries to resolve the package
    // - there is a conflict in the embedded request

    let repo = make_repo!(
        [
            {
                "pkg": "my-lib",
                "install": {
                    // python 2.7 requirement will conflict with the maya embedded one
                    "requirements": [{"pkg": "python/3.7"}, {"pkg": "maya/2020"}]
                },
            },
            {
                "pkg": "maya/2020",
                "install": {"embedded": [{"pkg": "python/2.7.5"}]},
            },
            {"pkg": "python/2.7.5"},
            {"pkg": "python/3.7.3"},
        ]
    );

    solver.add_repository(Arc::new(repo));
    solver.add_request(request!("python"));
    solver.add_request(request!("my-lib"));

    let res = io::run_and_print_resolve(&solver, 100);
    assert!(matches!(res, Err(Error::Solve(_))));
}

#[rstest]
fn test_solver_unknown_package_options(mut solver: Solver) {
    // test when a package is requested with specific options (eg: pkg.opt)
    // - the solver ignores versions that don't define the option
    // - the solver resolves versions that do define the option

    let repo = make_repo!([{"pkg": "my-lib/2.0.0"}]);
    let repo = Arc::new(repo);
    solver.add_repository(repo.clone());

    // this option is specific to the my-lib package and is not known by the package
    solver.add_request(request!({"var": "my-lib.something/value"}));
    solver.add_request(request!("my-lib"));

    let res = io::run_and_print_resolve(&solver, 100);
    assert!(matches!(res, Err(Error::Solve(_))));

    // this time we don't request that option, and it should be ok
    solver.reset();
    solver.add_repository(repo);
    solver.add_request(request!("my-lib"));
    io::run_and_print_resolve(&solver, 100).unwrap();
}

#[rstest]
fn test_solver_var_requirements(mut solver: Solver) {
    // test what happens when a dependency is added which is incompatible
    // with an existing request in the stack
    let repo = make_repo!(
        [
            {
                "pkg": "python/2.7.5",
                "build": {"options": [{"var": "abi", "static": "cp27mu"}]},
            },
            {
                "pkg": "python/3.7.3",
                "build": {"options": [{"var": "abi", "static": "cp37m"}]},
            },
            {
                "pkg": "my-app/1.0.0",
                "install": {
                    "requirements": [{"pkg": "python"}, {"var": "python.abi/cp27mu"}]
                },
            },
            {
                "pkg": "my-app/2.0.0",
                "install": {
                    "requirements": [{"pkg": "python"}, {"var": "python.abi/cp37m"}]
                },
            },
        ]
    );
    let repo = Arc::new(repo);

    solver.add_repository(repo.clone());
    solver.add_request(request!("my-app/2"));

    let solution = io::run_and_print_resolve(&solver, 100).unwrap();

    assert_resolved!(solution, "my-app", "2.0.0");
    assert_resolved!(solution, "python", "3.7.3");

    // requesting the older version of my-app should force old python abi
    solver.reset();
    solver.add_repository(repo);
    solver.add_request(request!("my-app/1"));

    let solution = io::run_and_print_resolve(&solver, 100).unwrap();

    assert_resolved!(solution, "python", "2.7.5");
}

#[rstest]
fn test_solver_var_requirements_unresolve(mut solver: Solver) {
    // test when a package is resolved that conflicts in var requirements
    //  - the solver should unresolve the solved package
    //  - the solver should resolve a new version of the package with the right version
    let repo = make_repo!(
        [
            {
                "pkg": "python/2.7.5",
                "build": {"options": [{"var": "abi", "static": "cp27"}]},
            },
            {
                "pkg": "python/3.7.3",
                "build": {"options": [{"var": "abi", "static": "cp37"}]},
            },
            {
                "pkg": "my-app/1.0.0",
                "install": {
                    "requirements": [{"pkg": "python"}, {"var": "python.abi/cp27"}]
                },
            },
            {
                "pkg": "my-app/2.0.0",
                "install": {"requirements": [{"pkg": "python"}, {"var": "abi/cp27"}]},
            },
        ]
    );
    let repo = Arc::new(repo);

    solver.add_repository(repo.clone());
    // python is resolved first to get 3.7
    solver.add_request(request!("python"));
    // the addition of this app constrains the python.abi to 2.7
    solver.add_request(request!("my-app/1"));

    let solution = io::run_and_print_resolve(&solver, 100).unwrap();

    assert_resolved!(solution, "my-app", "1.0.0");
    assert_resolved!(solution, "python", "2.7.5", "should re-resolve python");

    solver.reset();
    solver.add_repository(repo);
    // python is resolved first to get 3.7
    solver.add_request(request!("python"));
    // the addition of this app constrains the global abi to 2.7
    solver.add_request(request!("my-app/2"));

    let solution = io::run_and_print_resolve(&solver, 100).unwrap();

    assert_resolved!(solution, "my-app", "2.0.0");
    assert_resolved!(solution, "python", "2.7.5", "should re-resolve python");
}

#[rstest]
fn test_solver_build_options_dont_affect_compat(mut solver: Solver) {
    // test when a package is resolved with some build option
    //  - that option can conflict with another packages build options
    //  - as long as there is no explicit requirement on that option's value

    let dep_v1 = spec!({"pkg": "build-dep/1.0.0"});
    let dep_v2 = spec!({"pkg": "build-dep/2.0.0"});

    let a_spec = spec!({
        "pkg": "pkga/1.0.0",
        "build": {"options": [{"pkg": "build-dep/=1.0.0"}, {"var": "debug/on"}]},
    });

    let b_spec = spec!({
        "pkg": "pkgb/1.0.0",
        "build": {"options": [{"pkg": "build-dep/=2.0.0"}, {"var": "debug/off"}]},
    });

    let a_build = make_build!(a_spec, [dep_v1]);
    let b_build = make_build!(b_spec, [dep_v2]);
    let repo = make_repo!([a_build, b_build,]);
    repo.publish_spec(a_spec).unwrap();
    repo.publish_spec(b_spec).unwrap();
    let repo = Arc::new(repo);

    solver.add_repository(repo.clone());
    // a gets resolved and adds options for debug/on and build-dep/1
    // to the set of options in the solver
    solver.add_request(request!("pkga"));
    // b is not affected and can still be resolved
    solver.add_request(request!("pkgb"));

    io::run_and_print_resolve(&solver, 100).unwrap();

    solver.reset();
    solver.add_repository(repo.clone());
    solver.add_repository(repo.clone());
    solver.add_request(request!("pkga"));
    solver.add_request(request!("pkgb"));
    // this time the explicit request will cause a failure
    solver.add_request(request!({"var": "build-dep/=1.0.0"}));
    let res = io::run_and_print_resolve(&solver, 100);
    assert!(matches!(res, Err(Error::Solve(_))));
}

#[rstest]
fn test_solver_components(mut solver: Solver) {
    // test when a package is requested with specific components
    // - all the aggregated components are selected in the resolve
    // - the final build has published layers for each component

    let repo = make_repo!(
        [
            {
                "pkg": "python/3.7.3",
                "install": {
                    "components": [
                        {"name": "interpreter"},
                        {"name": "lib"},
                        {"name": "doc"},
                    ]
                },
            },
            {
                "pkg": "pkga",
                "install": {
                    "requirements": [{"pkg": "python:lib/3.7.3"}, {"pkg": "pkgb"}]
                },
            },
            {
                "pkg": "pkgb",
                "install": {"requirements": [{"pkg": "python:{doc,interpreter,run}"}]},
            },
        ]
    );

    solver.add_repository(Arc::new(repo));
    solver.add_request(request!("pkga"));
    solver.add_request(request!("pkgb"));

    let solution = io::run_and_print_resolve(&solver, 100).unwrap();

    let resolved = solution.get("python").unwrap().request.pkg.components;
    let expected = ["interpreter", "doc", "lib", "run"]
        .iter()
        .map(api::Component::parse)
        .map(Result::unwrap)
        .collect();
    assert_eq!(resolved, expected);
}

#[rstest]
fn test_solver_all_component(mut solver: Solver) {
    // test when a package is requested with the 'all' component
    // - all the specs components are selected in the resolve
    // - the final build has published layers for each component

    let repo = make_repo!(
        [
            {
                "pkg": "python/3.7.3",
                "install": {
                    "components": [
                        {"name": "bin", "uses": ["lib"]},
                        {"name": "lib"},
                        {"name": "doc"},
                        {"name": "dev", "uses": ["doc"]},
                    ]
                },
            },
        ]
    );

    solver.add_repository(Arc::new(repo));
    solver.add_request(request!("python:all"));

    let solution = io::run_and_print_resolve(&solver, 100).unwrap();

    let resolved = solution.get("python").unwrap();
    assert_eq!(resolved.request.pkg.components.len(), 1);
    assert_eq!(
        resolved.request.pkg.components.iter().next(),
        Some(&api::Component::All)
    );
    assert_resolved!(
        solution,
        "python",
        components = ["bin", "build", "dev", "doc", "lib", "run"]
    );
}

#[rstest]
fn test_solver_component_availability(mut solver: Solver) {
    // test when a package is requested with some component
    // - all the specs components are selected in the resolve
    // - the final build has published layers for each component

    let spec373 = spec!({
        "pkg": "python/3.7.3",
        "install": {
            "components": [
                {"name": "bin", "uses": ["lib"]},
                {"name": "lib"},
            ]
        },
    });
    let mut spec372 = spec373.clone();
    spec372.pkg = api::parse_ident("python/3.7.2").unwrap();
    let mut spec371 = spec373.clone();
    spec371.pkg = api::parse_ident("python/3.7.1").unwrap();

    // the first pkg has what we want on paper, but didn't actually publish
    // the components that we need (missing bin)
    let (build373, cmpt373) = make_build_and_components!(spec373, [], {}, ["lib"]);
    // the second pkg has what we request, but is missing a dependant component (lib)
    let (build372, cmpt372) = make_build_and_components!(spec372, [], {}, ["bin"]);
    // but the last/lowest version number has a publish for all components
    // and should be the one that is selected because of this
    let (build371, cmpt371) = make_build_and_components!(spec371, [], {}, ["bin", "lib"]);
    let repo = make_repo!([
        (build373, cmpt373),
        (build372, cmpt372),
        (build371, cmpt371),
    ]);
    repo.publish_spec(spec373).unwrap();
    repo.publish_spec(spec372).unwrap();
    repo.publish_spec(spec371).unwrap();

    solver.add_repository(Arc::new(repo));
    solver.add_request(request!("python:bin"));

    let solution = io::run_and_print_resolve(&solver, 100).unwrap();

    assert_resolved!(
        solution,
        "python",
        "3.7.1",
        "should resolve the only version with all the components we need actually published"
    );
    assert_resolved!(solution, "python", components = ["bin", "lib"]);
}

#[rstest]
fn test_solver_component_requirements(mut solver: Solver) {
    // test when a component has it's own list of requirements
    // - the requirements are added to the existing set of requirements
    // - the additional requirements are resolved
    // - even if it's a component that's only used by the one that was requested

    let repo = make_repo!(
        [
            {
                "pkg": "mypkg/1.0.0",
                "install": {
                    "requirements": [{"pkg": "dep"}],
                    "components": [
                        {"name": "build", "uses": ["build2"]},
                        {"name": "build2", "requirements": [{"pkg": "depb"}]},
                        {"name": "run", "requirements": [{"pkg": "depr"}]},
                    ],
                },
            },
            {"pkg": "dep"},
            {"pkg": "depb"},
            {"pkg": "depr"},
        ]
    );
    let repo = Arc::new(repo);

    solver.add_repository(repo.clone());
    solver.add_request(request!("mypkg:build"));

    let solution = io::run_and_print_resolve(&solver, 100).unwrap();

    solution.get("dep").expect("should exist");
    solution.get("depb").expect("should exist");
    assert!(solution.get("depr").is_none());

    solver.reset();
    solver.add_repository(repo);
    solver.add_request(request!("mypkg:run"));

    let solution = io::run_and_print_resolve(&solver, 100).unwrap();

    solution.get("dep").expect("should exist");
    solution.get("depr").expect("should exist");
    assert!(solution.get("depb").is_none());
}

#[rstest]
fn test_solver_component_requirements_extending(mut solver: Solver) {
    // test when an additional component is requested after a package is resolved
    // - the new components requirements are still added and resolved

    let repo = make_repo!(
        [
            {
                "pkg": "depa",
                "install": {
                    "components": [
                        {"name": "run", "requirements": [{"pkg": "depc"}]},
                    ],
                },
            },
            {"pkg": "depb", "install": {"requirements": [{"pkg": "depa:run"}]}},
            {"pkg": "depc"},
        ]
    );

    solver.add_repository(Arc::new(repo));
    // the initial resolve of this component will add no new requirements
    solver.add_request(request!("depa:build"));
    // depb has its own requirement on depa:run, which, also
    // has a new requirement on depc
    solver.add_request(request!("depb"));

    let solution = io::run_and_print_resolve(&solver, 100).unwrap();

    solution.get("depc").expect("should exist");
}

#[rstest]
fn test_solver_component_embedded(mut solver: Solver) {
    // test when a component has it's own list of embedded packages
    // - the embedded package is immediately selected
    // - it must be compatible with any previous requirements

    let repo = make_repo!(
        [
            {
                "pkg": "mypkg/1.0.0",
                "install": {
                    "components": [
                        {"name": "build", "embedded": [{"pkg": "dep-e1/1.0.0"}]},
                        {"name": "run", "embedded": [{"pkg": "dep-e2/1.0.0"}]},
                    ],
                },
            },
            {"pkg": "dep-e1/1.0.0"},
            {"pkg": "dep-e1/2.0.0"},
            {"pkg": "dep-e2/1.0.0"},
            {"pkg": "dep-e2/2.0.0"},
            {
                "pkg": "downstream1",
                "install": {
                    "requirements": [{"pkg": "dep-e1"}, {"pkg": "mypkg:build"}]
                },
            },
            {
                "pkg": "downstream2",
                "install": {
                    "requirements": [{"pkg": "dep-e2/2.0.0"}, {"pkg": "mypkg:run"}]
                },
            },
        ]
    );
    let repo = Arc::new(repo);

    solver.add_repository(repo.clone());
    solver.add_request(request!("downstream1"));

    let solution = io::run_and_print_resolve(&solver, 100).unwrap();

    assert_resolved!(solution, "dep-e1", build = Some(api::Build::Embedded));

    solver.reset();
    solver.add_repository(repo);
    solver.add_request(request!("downstream2"));

    // should fail because the one embedded package
    // does not meet the requirements in downstream spec
    let res = io::run_and_print_resolve(&solver, 100);
    assert!(matches!(res, Err(Error::Solve(_))));
}

#[rstest]
fn test_request_default_component() {
    let mut solver = Solver::default();
    let req = api::parse_ident("python/3.7.3").unwrap();
    solver.add_request(req.into());
    let state = solver.get_initial_state();
    let request = state
        .pkg_requests
        .get(0)
        .expect("solver should have a request");
    assert_eq!(
        request.pkg.components,
        vec![api::Component::Run].into_iter().collect(),
        "solver should inject a default run component if not otherwise given"
    )
}
