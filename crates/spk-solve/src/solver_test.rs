// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::sync::Arc;

use rstest::{fixture, rstest};
use spfs::encoding::EMPTY_DIGEST;
use spk_schema::foundation::fixtures::*;
use spk_schema::foundation::ident_build::Build;
use spk_schema::foundation::ident_component::Component;
use spk_schema::foundation::opt_name;
use spk_schema::ident::{
    build_ident,
    parse_ident_range,
    version_ident,
    PkgRequest,
    RangeIdent,
    Request,
    RequestedBy,
    VarRequest,
};
use spk_schema::ident_build::EmbeddedSource;
use spk_schema::prelude::*;
use spk_schema::{recipe, v0};
use spk_solve_solution::PackageSource;
use spk_storage::RepositoryHandle;

use super::{ErrorDetails, Solver};
use crate::io::DecisionFormatterBuilder;
use crate::{
    make_build,
    make_build_and_components,
    make_repo,
    option_map,
    request,
    spec,
    Error,
    Result,
};

#[fixture]
fn solver() -> Solver {
    Solver::default()
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
        assert_eq!(*pkg.spec.version(), $version, $message);
    }};

    ($solution:ident, $pkg:literal, build = $build:expr) => {
        assert_resolved!($solution, $pkg, build = $build, "wrong package build was resolved")
    };
    ($solution:ident, $pkg:literal, build = $build:expr, $message:literal) => {{
        let pkg = $solution
            .get($pkg)
            .expect("expected package to be in solution");
        assert_eq!(pkg.spec.ident().build(), &$build, $message);
    }};

    ($solution:ident, $pkg:literal, components = [$($component:literal),+ $(,)?]) => {{
        let mut resolved = std::collections::HashSet::<String>::new();
        let pkg = $solution
            .get($pkg)
            .expect("expected package to be in solution");
        match &pkg.source {
            PackageSource::Repository{components, ..} => {
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
        use $crate::Named;
        let names: std::collections::HashSet<_> = $solution
            .items()
            .into_iter()
            .map(|s| s.spec.name().to_string())
            .collect();
        let expected: std::collections::HashSet<_> = vec![
            $( $pkg.to_string() ),*
        ].into_iter().collect();
        assert_eq!(names, expected, "wrong set of packages was resolved");
    }};
}

/// Asserts that a package does not exist in the solution at any version.
macro_rules! assert_not_resolved {
    ($solution:ident, $pkg:literal) => {{
        let pkg = $solution.get($pkg);
        assert!(pkg.is_none());
    }};
}

/// Runs the given solver, printing the output with reasonable output settings
/// for unit test debugging and inspection.
async fn run_and_print_resolve_for_tests(solver: &Solver) -> Result<super::Solution> {
    let formatter = DecisionFormatterBuilder::new().with_verbosity(100).build();

    let (solution, _) = formatter.run_and_print_resolve(solver).await?;
    Ok(solution)
}

/// Runs the given solver, logging the output with reasonable output settings
/// for unit test debugging and inspection.
async fn run_and_log_resolve_for_tests(solver: &Solver) -> Result<super::Solution> {
    let formatter = DecisionFormatterBuilder::new().with_verbosity(100).build();

    let (solution, _) = formatter.run_and_log_resolve(solver).await?;
    Ok(solution)
}

#[rstest]
#[tokio::test]
async fn test_solver_no_requests(mut solver: Solver) {
    solver.solve().await.unwrap();
}

#[rstest]
#[tokio::test]
async fn test_solver_package_with_no_recipe(mut solver: Solver) {
    let repo = RepositoryHandle::new_mem();

    let options = option_map! {};
    let spec = v0::Spec::new(build_ident!(format!(
        "my-pkg/1.0.0/{}",
        options.digest_str()
    )));

    // publish package without publishing spec
    let components = vec![
        (Component::Run, EMPTY_DIGEST.into()),
        (Component::Build, EMPTY_DIGEST.into()),
    ]
    .into_iter()
    .collect();
    repo.publish_package(&spec.into(), &components)
        .await
        .unwrap();

    solver.update_options(options);
    solver.add_repository(Arc::new(repo));
    solver.add_request(request!("my-pkg"));

    // Test
    let res = run_and_print_resolve_for_tests(&solver).await;
    assert!(
        matches!(res, Ok(_)),
        "'{res:?}' should be an Ok(_) solution not an error.')"
    );
}

#[rstest]
#[tokio::test]
async fn test_solver_package_with_no_recipe_and_impossible_initial_checks(mut solver: Solver) {
    init_logging();
    let repo = RepositoryHandle::new_mem();

    let options = option_map! {};
    let spec = spec!({ "pkg": format!("my-pkg/1.0.0/{}", options.digest_str()) });

    // publish package without publishing spec
    let components = vec![(Component::Run, EMPTY_DIGEST.into())]
        .into_iter()
        .collect();
    repo.publish_package(&spec, &components).await.unwrap();

    solver.update_options(options);
    solver.add_repository(Arc::new(repo));
    solver.add_request(request!("my-pkg"));
    solver.set_initial_request_impossible_checks(true);

    // Test
    let res = run_and_print_resolve_for_tests(&solver).await;
    if cfg!(feature = "migration-to-components") {
        match res {
            Err(Error::InitialRequestsContainImpossibleError(_)) => {
                // Success, when the 'migration-to-components' feature
                // is enabled because the initial checks for
                // impossible requests fail because the package does
                // not have a :build component, it only has a :run
                // component and the request was transformed into
                // my-pkg:all, which requires :build and :run to pass
                // validation under the feature.
            }
            Err(err) => panic!("expected a solver Error::String error, got: {err}"),
            Ok(_) => panic!("expected a solver Error::String error, got an Ok(_) solution"),
        }
    } else {
        match res {
            Ok(_) => {
                // Success, when the 'migration-to-components' feature is
                // disabled because: the initial checks for impossible
                // requests pass and this allows the solver to run and
                // find a solution.
            }
            Err(err) => panic!("expected an Ok(_) soluation, got: {err}"),
        }
    }
}

#[rstest]
#[tokio::test]
async fn test_solver_package_with_no_recipe_from_cmd_line(mut solver: Solver) {
    let repo = RepositoryHandle::new_mem();

    let spec = spec!({"pkg": "my-pkg/1.0.0/4OYMIQUY"});

    // publish package without publishing recipe
    let components = vec![
        (Component::Run, EMPTY_DIGEST.into()),
        (Component::Build, EMPTY_DIGEST.into()),
    ]
    .into_iter()
    .collect();
    repo.publish_package(&spec, &components).await.unwrap();

    solver.add_repository(Arc::new(repo));
    // Create this one as requested by the command line, rather than the tests
    let req = Request::Pkg(PkgRequest::new(
        parse_ident_range("my-pkg").unwrap(),
        RequestedBy::CommandLine,
    ));
    solver.add_request(req);

    // Test
    let res = run_and_print_resolve_for_tests(&solver).await;
    assert!(
        matches!(res, Ok(_)),
        "'{res:?}' should be an Ok(_) solution not an error.')"
    );
}

#[rstest]
#[tokio::test]
async fn test_solver_package_with_no_recipe_from_cmd_line_and_impossible_initial_checks(
    mut solver: Solver,
) {
    init_logging();
    let repo = RepositoryHandle::new_mem();

    let spec = spec!({"pkg": "my-pkg/1.0.0/4OYMIQUY"});

    // publish package without publishing recipe
    let components = vec![(Component::Run, EMPTY_DIGEST.into())]
        .into_iter()
        .collect();
    repo.publish_package(&spec, &components).await.unwrap();

    solver.add_repository(Arc::new(repo));
    // Create this one as requested by the command line, rather than the tests
    let req = Request::Pkg(PkgRequest::new(
        parse_ident_range("my-pkg").unwrap(),
        RequestedBy::CommandLine,
    ));
    solver.add_request(req);
    solver.set_initial_request_impossible_checks(true);

    // Test
    let res = run_and_print_resolve_for_tests(&solver).await;
    if cfg!(feature = "migration-to-components") {
        // with the 'migration-to-components' feature and impossible
        // request initial checks will fail because the feature turns
        // the initial request into my-pkg:all, which requires a
        // :build and a :run component to pass and it only has a :run
        // component
        assert!(
            matches!(res, Err(Error::InitialRequestsContainImpossibleError(_))),
            "'{res:?}' should be a Error::String('Initial requests contain 1 impossible request.')",
        );
    } else {
        // without the 'migration-to-components' feature, the
        // impossible request initial checks will succeed because
        // because the initial request is turned into my-pkg:run,
        // which will pass validation
        assert!(
            matches!(res, Ok(_)),
            "'{res:?}' should be an Ok(_) solution not an error.')",
        );
    }
}

#[rstest]
#[tokio::test]
async fn test_solver_single_package_no_deps(mut solver: Solver) {
    let options = option_map! {};
    let repo = make_repo!([{"pkg": "my-pkg/1.0.0"}], options=options.clone());

    solver.update_options(options);
    solver.add_repository(Arc::new(repo));
    solver.add_request(request!("my-pkg"));

    let packages = run_and_print_resolve_for_tests(&solver).await.unwrap();
    assert_eq!(packages.len(), 1, "expected one resolved package");
    let resolved = packages.get("my-pkg").unwrap();
    assert_eq!(&resolved.spec.version().to_string(), "1.0.0");
    assert_ne!(resolved.spec.ident().build(), &Build::Source);
}

#[rstest]
#[tokio::test]
async fn test_solver_single_package_simple_deps(mut solver: Solver) {
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

    let packages = run_and_print_resolve_for_tests(&solver).await.unwrap();
    assert_eq!(packages.len(), 2, "expected two resolved packages");
    assert_resolved!(packages, "pkg-a", "1.2.1");
    assert_resolved!(packages, "pkg-b", "1.1.0");
}

#[rstest]
#[tokio::test]
async fn test_solver_dependency_abi_compat(mut solver: Solver) {
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

    let packages = run_and_print_resolve_for_tests(&solver).await.unwrap();
    assert_eq!(packages.len(), 2, "expected two resolved packages");
    assert_resolved!(packages, "pkg-a", "1.1.1");
    assert_resolved!(packages, "pkg-b", "1.1.0");
}

#[rstest]
#[tokio::test]
async fn test_solver_dependency_incompatible(mut solver: Solver) {
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

    let res = run_and_print_resolve_for_tests(&solver).await;

    assert!(res.is_err());
}

#[rstest]
#[tokio::test]
async fn test_solver_dependency_incompatible_stepback(mut solver: Solver) {
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

    let packages = run_and_print_resolve_for_tests(&solver).await.unwrap();

    assert_resolved!(packages, "my-plugin", "1.0.0");
    assert_resolved!(packages, "maya", "2019.0.0");
}

#[rstest]
#[tokio::test]
async fn test_solver_dependency_already_satisfied(mut solver: Solver) {
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

    let packages = run_and_print_resolve_for_tests(&solver).await.unwrap();

    assert_resolved!(packages, ["pkg-top", "dep-1", "dep-2"]);
    assert_resolved!(packages, "dep-1", "1.0.0");
}

#[rstest]
#[tokio::test]
async fn test_solver_dependency_reopen_solvable(mut solver: Solver) {
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

    let packages = run_and_print_resolve_for_tests(&solver).await.unwrap();
    assert_resolved!(packages, ["my-plugin", "some-library", "maya"]);
    assert_resolved!(packages, "maya", "2019.0.0");
}

#[rstest]
#[tokio::test]
async fn test_solver_dependency_reiterate(mut solver: Solver) {
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

    let packages = run_and_print_resolve_for_tests(&solver).await.unwrap();
    assert_resolved!(packages, ["my-plugin", "some-library", "maya"]);
    assert_resolved!(packages, "maya", "2019.0.0");
}

#[rstest]
#[tokio::test]
async fn test_solver_dependency_reopen_unsolvable(mut solver: Solver) {
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

    let result = run_and_print_resolve_for_tests(&solver).await;
    assert!(result.is_err());
}

#[rstest]
#[tokio::test]
async fn test_solver_pre_release_config(mut solver: Solver) {
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

    let solution = run_and_print_resolve_for_tests(&solver).await.unwrap();
    assert_resolved!(
        solution,
        "my-pkg",
        "0.9.0",
        "should not resolve pre-release by default"
    );

    solver.reset();
    solver.add_repository(repo);
    solver.add_request(request!({"pkg": "my-pkg", "prereleasePolicy": "IncludeAll"}));

    let solution = run_and_print_resolve_for_tests(&solver).await.unwrap();
    assert_resolved!(solution, "my-pkg", "1.0.0-pre.2");
}

#[rstest]
#[tokio::test]
async fn test_solver_constraint_only(mut solver: Solver) {
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

    let solution = run_and_print_resolve_for_tests(&solver).await.unwrap();
    assert!(solution.get("python").is_none());
}

#[rstest]
#[tokio::test]
async fn test_solver_constraint_and_request(mut solver: Solver) {
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

    let solution = run_and_print_resolve_for_tests(&solver).await.unwrap();

    assert_resolved!(solution, "python", "3.7.3");
}

#[rstest]
#[tokio::test]
async fn test_solver_option_compatibility(mut solver: Solver) {
    // test what happens when an option is given in the solver
    // - the options for each build are checked
    // - the resolved build must have used the option

    let spec = recipe!(
        {
            "pkg": "vnp3/2.0.0",
            "build": {
                // The 'by_distance' build sorting method relied on this for the
                // tests to pass:
                // favoritize 2.7, otherwise an option of python=2 doesn't actually
                // exclude python 3 from being resolved
                "options": [{"pkg": "python/~2.7"}],
                "variants": [{"python": "3.7"}, {"python": "2.7"}],
            },
        }
    );
    let py27 = make_build!({"pkg": "python/2.7.5"});
    let py26 = make_build!({"pkg": "python/2.6"});
    let py371 = make_build!({"pkg": "python/3.7.1"});
    let py37 = make_build!({"pkg": "python/3.7.3"});

    let for_py27 = make_build!(spec, [py27]);
    let for_py26 = make_build!(spec, [py26]);
    let for_py371 = make_build!(spec, [py371]);
    let for_py37 = make_build!(spec, [py37]);
    let repo = make_repo!([for_py27, for_py26, for_py37, for_py371]);
    repo.publish_recipe(&spec).await.unwrap();
    let repo = Arc::new(repo);

    // The 'by_build_option_values' build sorting method does not use
    // the variants or recipe's default options. It sorts the
    // builds by putting the ones with the highest numbered build
    // options (dependencies) first. The '~'s and ',<3's have been
    // added to some of the version ranges below force the solver to
    // work through the ordered builds until it finds an appropriate
    // 2.x.y values to both solve and pass the test.
    for pyver in [
        // Uncomment this, when the '2,<3' parsing bug: https://github.com/imageworks/spk/issues/322 has been fixed
        //"~2.0", "~2.7", "~2.7.5", "2,<3", "2.7,<3", "3", "3.7", "3.7.3",
        "~2.0", "~2.7", "~2.7.5", "3", "3.7", "3.7.3",
    ] {
        solver.reset();
        solver.add_repository(repo.clone());
        solver.add_request(request!("vnp3"));
        solver.add_request(
            VarRequest {
                var: opt_name!("python").to_owned(),
                pin: false,
                value: pyver.to_string(),
            }
            .into(),
        );

        let solution = run_and_print_resolve_for_tests(&solver).await.unwrap();

        let resolved = solution.get("vnp3").unwrap();
        let opt = resolved.spec.options().get(0).unwrap();
        let value = opt.get_value(None);

        // Check the first digit component of the pyver value
        let expected = if pyver.starts_with('~') {
            format!("~{}", pyver.chars().nth(1).unwrap()).to_string()
        } else {
            format!("~{}", pyver.chars().next().unwrap()).to_string()
        };
        assert!(
            value.starts_with(&expected),
            "{value} should start with ~{expected} to be valid for {pyver}"
        );
    }
}

#[rstest]
#[tokio::test]
async fn test_solver_option_injection(mut solver: Solver) {
    // test the options that are defined when a package is resolved
    // - options are namespaced and added to the environment
    init_logging();
    let spec = recipe!(
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
    repo.publish_recipe(&spec).await.unwrap();

    solver.add_repository(Arc::new(repo));
    solver.add_request(request!("vnp3"));

    let solution = run_and_print_resolve_for_tests(&solver).await.unwrap();

    let mut opts = solution.options().clone();
    assert_eq!(opts.remove(opt_name!("vnp3")), Some("~2.0.0".to_string()));
    assert_eq!(
        opts.remove(opt_name!("vnp3.python")),
        Some("~2.7.5".to_string())
    );
    assert_eq!(opts.remove(opt_name!("vnp3.debug")), Some("on".to_string()));
    assert_eq!(
        opts.remove(opt_name!("python.abi")),
        Some("cp27mu".to_string())
    );
    assert!(
        !opts.contains_key(opt_name!("vnp3.special")),
        "should not define empty values"
    );
    assert_eq!(opts.len(), 0, "expected no more options, got {opts}");
}

#[rstest]
#[tokio::test]
async fn test_solver_build_from_source(mut solver: Solver) {
    init_logging();
    // test when no appropriate build exists but the source is available
    // - the build is skipped
    // - the source package is checked for current options
    // - a new build is created
    // - the local package is used in the resolve

    let repo = make_repo!(
        [
            {
                "pkg": "my-tool/1.2.0/src",
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
    solver.set_binary_only(false);
    // the new option value should disqualify the existing build
    // but a new one should be generated for this set of options
    solver.add_request(request!({"var": "debug/on"}));
    solver.add_request(request!("my-tool"));

    let solution = run_and_print_resolve_for_tests(&solver).await.unwrap();

    let resolved = solution.get("my-tool").unwrap();
    assert!(
        resolved.is_source_build(),
        "Should set unbuilt spec as source: {}",
        resolved.spec.ident()
    );

    solver.reset();
    solver.add_repository(repo);
    solver.add_request(request!({"var": "debug/on"}));
    solver.add_request(request!("my-tool"));
    solver.set_binary_only(true);
    // Should fail when binary-only is specified

    let res = run_and_print_resolve_for_tests(&solver).await;

    assert!(res.is_err());
}

#[rstest]
#[tokio::test]
async fn test_solver_build_from_source_unsolvable(mut solver: Solver) {
    let log = init_logging();
    // test when no appropriate build exists but the source is available
    // - if the requested pkg cannot resolve a build environment
    // - this is flagged by the solver as impossible

    let gcc48 = make_build!({"pkg": "gcc/4.8"});
    let recipe = spk_schema::recipe!({
        "pkg": "my-tool/1.2.0",
        "build": {"options": [{"pkg": "gcc"}], "script": "echo BUILD"},
    });
    let build_with_48 = make_build!(recipe, [gcc48]);
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
    // the macro adds a recipe for the source package, but we want
    // to have the actual recipe that can generate a valid build with
    // more than just a src component
    // TODO: why is this the case, can we avoid this in the macro?
    repo.remove_recipe(recipe.ident()).await.ok();
    repo.publish_recipe(&recipe).await.unwrap();

    solver.add_repository(Arc::new(repo));
    // the new option value should disqualify the existing build
    // and there is no 6.3 that can be resolved for this request
    solver.add_request(request!({"var": "gcc/6.3"}));
    solver.add_request(request!("my-tool:run"));

    let res = run_and_log_resolve_for_tests(&solver).await;

    assert!(res.is_err(), "should fail to resolve");
    let log = log.lock();
    let event = log.all_events().find(|e| {
        let Some(msg) = e.message() else {
            return false;
        };
        let Ok(msg) = strip_ansi_escapes::strip(msg) else {
            return false;
        };
        let msg = String::from_utf8_lossy(&msg);
        msg.ends_with("TRY my-tool/1.2.0/src - cannot resolve build env: Failed to resolve")
    });
    assert!(
        event.is_some(),
        "should block because of failed build env resolve"
    );
}

#[rstest]
#[tokio::test]
async fn test_solver_build_from_source_dependency(mut solver: Solver) {
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
            // the source package needs to exist for the build to be possible
            {
                "pkg": "my-tool/1.2.0/src",
            },
            // one existing build exists that used python 3.6.3
            build_with_py36,
            // only python 3.7 exists, which is api compatible, but not abi
            {"pkg": "python/3.7.3", "compat": "x.a.b"},
        ]
    );
    // the actual recipe pins from the build env and so can satisfy
    // building against the newer build of python
    let recipe = recipe!({
        "pkg": "my-tool/1.2.0",
        "build": {"options": [{"pkg": "python"}]},
        "install": {
            "requirements": [{"pkg": "python", "fromBuildEnv": "x.x.x"}]
        },
    });
    repo.force_publish_recipe(&recipe).await.unwrap();

    // the new option value should disqualify the existing build
    // but a new one should be generated for this set of options
    solver.update_options(option_map! {"debug" => "on"});
    solver.add_repository(Arc::new(repo));
    solver.add_request(request!("my-tool"));
    solver.set_binary_only(false);

    let solution = run_and_print_resolve_for_tests(&solver).await.unwrap();

    assert!(
        solution.get("my-tool").unwrap().is_source_build(),
        "should want to build"
    );
}

#[rstest]
#[tokio::test]
async fn test_solver_deprecated_build(mut solver: Solver) {
    let deprecated = make_build!({"pkg": "my-pkg/1.0.0", "deprecated": true});
    let deprecated_build = deprecated.ident().clone();
    let repo = make_repo!([
        {"pkg": "my-pkg/0.9.0"},
        {"pkg": "my-pkg/1.0.0"},
        deprecated,
    ]);
    let repo = Arc::new(repo);

    solver.add_repository(repo.clone());
    solver.add_request(request!("my-pkg"));

    let solution = run_and_print_resolve_for_tests(&solver).await.unwrap();
    assert_resolved!(
        solution,
        "my-pkg",
        "0.9.0",
        "should not resolve deprecated build by default"
    );

    solver.reset();
    solver.add_repository(repo);
    solver.add_request(
        PkgRequest::from_ident(deprecated_build.to_any(), RequestedBy::SpkInternalTest).into(),
    );

    let solution = run_and_print_resolve_for_tests(&solver).await.unwrap();
    assert_resolved!(
        solution,
        "my-pkg",
        "1.0.0",
        "should be able to resolve exact deprecated build"
    );
}

#[rstest]
#[tokio::test]
async fn test_solver_deprecated_version(mut solver: Solver) {
    let deprecated = make_build!({"pkg": "my-pkg/1.0.0", "deprecated": true});
    let repo = make_repo!(
        [{"pkg": "my-pkg/0.9.0"}, {"pkg": "my-pkg/1.0.0", "deprecated": true}, deprecated]
    );
    let repo = Arc::new(repo);

    solver.add_repository(repo.clone());
    solver.add_request(request!("my-pkg"));

    let solution = run_and_print_resolve_for_tests(&solver).await.unwrap();
    assert_resolved!(
        solution,
        "my-pkg",
        "0.9.0",
        "should not resolve build when version is deprecated by default"
    );

    solver.reset();
    solver.add_repository(repo);
    solver.add_request(
        PkgRequest::new(
            RangeIdent::equals(&deprecated.ident().to_any(), []),
            RequestedBy::SpkInternalTest,
        )
        .into(),
    );

    let solution = run_and_print_resolve_for_tests(&solver).await.unwrap();
    assert_resolved!(
        solution,
        "my-pkg",
        "1.0.0",
        "should be able to resolve exact build when version is deprecated"
    );
}

#[rstest]
#[tokio::test]
async fn test_solver_build_from_source_deprecated(mut solver: Solver) {
    // test when no appropriate build exists and the main package
    // has been deprecated, no source build should be allowed

    let repo = make_repo!(
        [
            {
                "pkg": "my-tool/1.2.0/src",
                "deprecated": false,
                "build": {"options": [{"var": "debug"}], "script": "echo BUILD"},
            },
            {
                "pkg": "my-tool/1.2.0",
                "deprecated": true,
                "build": {"options": [{"var": "debug"}], "script": "echo BUILD"},
            },
        ],
        options = {"debug" => "off"}
    );
    let spec = repo
        .read_recipe(&version_ident!("my-tool/1.2.0"))
        .await
        .unwrap();
    repo.force_publish_recipe(&spec).await.unwrap();

    solver.add_repository(Arc::new(repo));
    solver.add_request(request!({"var": "debug/on"}));
    solver.add_request(request!("my-tool"));

    let res = run_and_print_resolve_for_tests(&solver).await;
    match res {
        Err(Error::GraphError(spk_solve_graph::Error::FailedToResolve(_))) => {}
        Err(err) => {
            panic!("expected solver spk_solver_graph::Error::FailedToResolve, got: '{err:?}'")
        }
        _ => panic!("expected a solver error, got successful solution"),
    }
}

#[rstest]
#[tokio::test]
async fn test_solver_build_from_source_deprecated_and_impossible_initial_checks(
    mut solver: Solver,
) {
    // test when no appropriate build exists and the main package
    // has been deprecated, no source build should be allowed
    init_logging();
    let repo = make_repo!(
        [
            {
                "pkg": "my-tool/1.2.0/src",
                "deprecated": false,
                "build": {"options": [{"var": "debug"}], "script": "echo BUILD"},
            },
            {
                "pkg": "my-tool/1.2.0",
                "deprecated": true,
                "build": {"options": [{"var": "debug"}], "script": "echo BUILD"},
            },
        ],
        options = {"debug" => "off"}
    );
    let spec = repo
        .read_recipe(&version_ident!("my-tool/1.2.0"))
        .await
        .unwrap();
    repo.force_publish_recipe(&spec).await.unwrap();

    solver.add_repository(Arc::new(repo));
    solver.add_request(request!({"var": "debug/on"}));
    solver.add_request(request!("my-tool"));
    solver.set_initial_request_impossible_checks(true);

    let res = run_and_print_resolve_for_tests(&solver).await;
    match res {
        Err(Error::GraphError(spk_solve_graph::Error::FailedToResolve(_))) => {
            // Success, when the 'migration-to-components' feature is
            // enabled because: the initial checks for impossible
            // requests pass because the :all component matches the
            // :src component of the non-deprecated build and this allows
            // the solver to run. The solver finds the package/verison
            // recipe is deprecated and refuses to build a binary from
            // the source package.
        }
        Err(Error::InitialRequestsContainImpossibleError(_)) => {
            // Success, when the 'migration-to-components' feature is
            // disabled because: the initial checks for impossible
            // requests fail to find a possible build because the
            // default :run component does not match the :src
            // component of the non-deprecated package
        }
        Err(err) => {
            panic!("expected different solver error, got: '{err:?}'")
        }
        _ => panic!("expected a solver error, got a successful solution"),
    }
}

#[rstest]
#[tokio::test]
async fn test_solver_embedded_package_adds_request(mut solver: Solver) {
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

    let solution = run_and_print_resolve_for_tests(&solver).await.unwrap();

    assert_resolved!(
        solution,
        "qt",
        build = Build::Embedded(EmbeddedSource::Unknown)
    );
    assert_resolved!(solution, "qt", "5.12.6");
    assert_resolved!(
        solution,
        "qt",
        build = Build::Embedded(EmbeddedSource::Unknown)
    );
}

#[rstest]
#[tokio::test]
async fn test_solver_embedded_package_solvable(mut solver: Solver) {
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

    let solution = run_and_print_resolve_for_tests(&solver).await.unwrap();

    assert_resolved!(solution, "qt", "5.12.6");
    assert_resolved!(
        solution,
        "qt",
        build = Build::Embedded(EmbeddedSource::Unknown)
    );
}

#[rstest]
#[tokio::test]
async fn test_solver_embedded_package_unsolvable(mut solver: Solver) {
    // test when there is an embedded package
    // - the embedded package is added to the solution
    // - the embedded package conflicts with existing requests

    let repo = make_repo!(
        [
            {
                "pkg": "my-plugin",
                // the qt/5.13 requirement is available but conflicts with maya embedded
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

    let res = run_and_print_resolve_for_tests(&solver).await;
    assert!(res.is_err());
}

#[rstest]
#[tokio::test]
async fn test_solver_embedded_package_replaces_real_package(mut solver: Solver) {
    // test when there is an embedded package
    // - the embedded package is added to the solution
    // - any dependencies from the "real" package aren't part of the solution

    init_logging();
    let repo = make_repo!(
        [
            {
                "pkg": "unwanted-dep",
            },
            {
                "pkg": "thing-needs-plugin",
                "install": {"requirements": [{"pkg": "my-plugin"}]},
            },
            {
                "pkg": "my-plugin",
                "install": {"requirements": [
                    // Try to resolve qt first -- this should find the "real"
                    // package first.
                    {"pkg": "qt/5.12.6"},
                    {"pkg": "maya/2019"}
                ]},
            },
            {
                "pkg": "maya/2019.2",
                "install": {"embedded": [{"pkg": "qt/5.12.6"}]},
            },
            {
                "pkg": "qt/5.12.6", // same version as embedded
                "install": {"requirements": [{"pkg": "unwanted-dep"}]},
            },
        ]
    );

    solver.add_repository(Arc::new(repo));
    // Add qt to the request so "unwanted-dep" becomes part of the solution
    // temporarily.
    solver.add_request(request!("qt"));
    // Can't directly request "my-plugin" or it gets resolved before
    // "unwanted-dep" is added to solution.
    solver.add_request(request!("thing-needs-plugin"));

    let solution = run_and_print_resolve_for_tests(&solver).await.unwrap();

    // At time of writing, this is a point where "unwanted-dep" is part of the
    // solution:
    //    State Resolved: qt/5.12.6/3I42H3S6, thing-needs-plugin/0/3I42H3S6, unwanted-dep/0/3I42H3S6

    assert_resolved!(solution, "qt", "5.12.6");
    assert_resolved!(
        solution,
        "qt",
        build = Build::Embedded(EmbeddedSource::Unknown)
    );
    assert_not_resolved!(solution, "unwanted-dep");
}

#[rstest]
#[tokio::test]
async fn test_solver_initial_request_impossible_masks_embedded_package_solution(
    mut solver: Solver,
) {
    // test when an embedded package and its parent package are
    // requested and impossible checks are enabled for initial
    // requests
    // - the embedded package will be found during the impossible checks
    // - the solver will find a solution using the embedded package
    init_logging();

    // Needs a repo with an embedded package, it's parent package, and
    // a non-embedded different version of the same package name as
    // the embedded package.
    let repo = make_repo!(
        [
            {
                "pkg": "maya/2019.2",
                "install": {"embedded": [{"pkg": "qt/5.12.6"}]},
            },
            {
                "pkg": "qt/5.13.0",
            },
        ]
    );

    solver.add_repository(Arc::new(repo));
    // Ask for the embedded qt package first to ensure the embedded
    // package support and the impossible checks on the initial
    // requests work correctly.
    solver.add_request(request!("qt/5.12.6"));
    solver.add_request(request!("maya"));
    solver.set_initial_request_impossible_checks(true);

    match run_and_print_resolve_for_tests(&solver).await {
        Ok(solution) => {
            assert_resolved!(solution, "qt", "5.12.6");
            assert_resolved!(
                solution,
                "qt",
                build = Build::Embedded(EmbeddedSource::Unknown)
            );
        }
        Err(err) => {
            panic!("Expected a solution but solver errored with: {err}");
        }
    };
}

#[rstest]
#[tokio::test]
async fn test_solver_impossible_request_but_embedded_package_makes_solvable(mut solver: Solver) {
    // test when there is an embedded package
    // - the initial request depends on the same package as the embedded package
    // - an impossible request is found for the same package first
    // - one of the dependency branches leads to a package that has an embedded package that
    //   resolves the first requests
    // - the solution includes the embedded packages

    // needs/1.0.0 -> something
    //             -> somethingelse
    // something/2.4.0 -> maya
    // somethingelse/3.2.1 -> qt/5.12.6
    // qt/5.13.0
    // maya/2019.2 -> embeds qt/5.12.6
    init_logging();

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
            {
                "pkg": "something/2.4.0",
                "build": {"script": "echo BUILD"},
                "install": {"requirements": [ {"pkg": "maya"}]},
            },
            {
                "pkg": "somethingelse/3.2.1",
                "build": {"script": "echo BUILD"},
                "install": {"requirements": [ {"pkg": "qt/5.12.6"}]},
            },
            {
                "pkg": "needs/1.0.0",
                "build": {"script": "echo BUILD"},
                "install": {"requirements": [ {"pkg": "something"}, {"pkg": "somethingelse"}]},
            }
        ]
    );

    solver.add_repository(Arc::new(repo));
    solver.add_request(request!("needs"));
    solver.set_resolve_validation_impossible_checks(true);

    // The solutions is: needs/1.0.0 -> something/2.4.0 -> maya/2019.2 (embeds qt/5.12.6)
    //                               -> somethingelse/3.2.1 ----------------------^
    //
    // But the request for 'somethingelse' (made by needs/1.0.0) will
    // be found to generate an impossible request because there's no
    // qt that matches 5.12.6 among the non-embedded published
    // packages. And the lack of other choices will halt the search at
    // that point because the solver does not process all unresolved
    // requests before stopping and this is not an embedded package
    // cache for it to check.
    match run_and_print_resolve_for_tests(&solver).await {
        Ok(solution) => {
            assert_resolved!(solution, "qt", "5.12.6");
            assert_resolved!(
                solution,
                "qt",
                build = Build::Embedded(EmbeddedSource::Unknown)
            );
        }
        Err(err) => {
            // This should not happen
            panic!("Expected a solution, but got this error: {err}");
        }
    };
}

#[rstest]
#[tokio::test]
async fn test_solver_with_impossible_checks_in_build_keys(mut solver: Solver) {
    let options1 = option_map! {"dep" => "1.0.0"};
    let options2 = option_map! {"dep" => "2.0.0"};

    let dep1 = spec!({"pkg": "dep/1.0.0/3I42H3S6"});
    let dep2 = spec!({"pkg": "dep/2.0.0/3I42H3S6"});

    let a_spec = recipe!({
        "pkg": "pkg-a/1.0.0",
        "build": {"options": [{"pkg": "dep/1.0.0"}],
                  "variants": [{"pkg": "dep/=1.0.0"}, {"pkg": "dep/=2.0.0"}],
        },
        "install": {"requirements": [{"pkg": "dep", "fromBuildEnv": "x.x.x"}]},
    });

    let build1 = make_build!(a_spec, [dep1], options1);
    let build2 = make_build!(a_spec, [dep2], options2);
    // dep2 is deliberately not in the repo to generate an impossible
    // request when build2 is examined
    let repo = make_repo!([{"pkg": "pkg-top/1.2.3",
                            "install": { "requirements": [{"pkg": "pkg-a"}] }},
                           {"pkg": "dep/1.0.0"},
                           build1,
                           build2]);
    repo.publish_recipe(&a_spec).await.unwrap();

    solver.add_repository(Arc::new(repo));
    solver.add_request(request!("pkg-top"));
    // This is to exercise the check. The missing dep2 package will
    // ensure that the package that depends on dep1 is chosen.
    solver.set_build_key_impossible_checks(true);

    let packages = run_and_print_resolve_for_tests(&solver).await.unwrap();
    assert_resolved!(packages, "pkg-a", "1.0.0");
    assert_resolved!(packages, "dep", "1.0.0");
}

#[rstest]
#[tokio::test]
async fn test_solver_some_versions_conflicting_requests(mut solver: Solver) {
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

    let solution = run_and_print_resolve_for_tests(&solver).await.unwrap();

    assert_resolved!(solution, "dep", "2.0.0");
}

#[rstest]
#[tokio::test]
async fn test_solver_embedded_request_invalidates(mut solver: Solver) {
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

    let res = run_and_print_resolve_for_tests(&solver).await;

    assert!(res.is_err());
}

#[rstest]
#[tokio::test]
async fn test_solver_unknown_package_options(mut solver: Solver) {
    // test when a package is requested with specific options (eg: pkg.opt)
    // - the solver ignores versions that don't define the option
    // - the solver resolves versions that do define the option

    let repo = make_repo!([{"pkg": "my-lib/2.0.0"}]);
    let repo = Arc::new(repo);
    solver.add_repository(repo.clone());

    // this option is specific to the my-lib package and is not known by the package
    solver.add_request(request!({"var": "my-lib.something/value"}));
    solver.add_request(request!("my-lib"));

    let res = run_and_print_resolve_for_tests(&solver).await;
    assert!(res.is_err());

    // this time we don't request that option, and it should be ok
    solver.reset();
    solver.add_repository(repo);
    solver.add_request(request!("my-lib"));
    run_and_print_resolve_for_tests(&solver).await.unwrap();
}

#[rstest]
#[tokio::test]
async fn test_solver_var_requirements(mut solver: Solver) {
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

    let solution = run_and_print_resolve_for_tests(&solver).await.unwrap();

    assert_resolved!(solution, "my-app", "2.0.0");
    assert_resolved!(solution, "python", "3.7.3");

    // requesting the older version of my-app should force old python abi
    solver.reset();
    solver.add_repository(repo);
    solver.add_request(request!("my-app/1"));

    let solution = run_and_print_resolve_for_tests(&solver).await.unwrap();

    assert_resolved!(solution, "python", "2.7.5");
}

#[rstest]
#[tokio::test]
async fn test_solver_var_requirements_unresolve(mut solver: Solver) {
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

    let solution = run_and_print_resolve_for_tests(&solver).await.unwrap();

    assert_resolved!(solution, "my-app", "1.0.0");
    assert_resolved!(solution, "python", "2.7.5", "should re-resolve python");

    solver.reset();
    solver.add_repository(repo);
    // python is resolved first to get 3.7
    solver.add_request(request!("python"));
    // the addition of this app constrains the global abi to 2.7
    solver.add_request(request!("my-app/2"));

    let solution = run_and_print_resolve_for_tests(&solver).await.unwrap();

    assert_resolved!(solution, "my-app", "2.0.0");
    assert_resolved!(solution, "python", "2.7.5", "should re-resolve python");
}

#[rstest]
#[tokio::test]
async fn test_solver_build_options_dont_affect_compat(mut solver: Solver) {
    // test when a package is resolved with some build option
    //  - that option can conflict with another packages build options
    //  - as long as there is no explicit requirement on that option's value

    let dep_v1 = spec!({"pkg": "build-dep/1.0.0/3I42H3S6"});
    let dep_v2 = spec!({"pkg": "build-dep/2.0.0/3I42H3S6"});

    let a_spec = recipe!({
        "pkg": "pkga/1.0.0",
        "build": {"options": [{"pkg": "build-dep/=1.0.0"}, {"var": "debug/on"}]},
    });

    let b_spec = recipe!({
        "pkg": "pkgb/1.0.0",
        "build": {"options": [{"pkg": "build-dep/=2.0.0"}, {"var": "debug/off"}]},
    });

    let a_build = make_build!(a_spec, [dep_v1]);
    let b_build = make_build!(b_spec, [dep_v2]);
    let repo = make_repo!([a_build, b_build,]);
    repo.publish_recipe(&a_spec).await.unwrap();
    repo.publish_recipe(&b_spec).await.unwrap();
    let repo = Arc::new(repo);

    solver.add_repository(repo.clone());
    // a gets resolved and adds options for debug/on and build-dep/1
    // to the set of options in the solver
    solver.add_request(request!("pkga"));
    // b is not affected and can still be resolved
    solver.add_request(request!("pkgb"));

    run_and_print_resolve_for_tests(&solver).await.unwrap();

    solver.reset();
    solver.add_repository(repo.clone());
    solver.add_repository(repo);
    solver.add_request(request!("pkga"));
    solver.add_request(request!("pkgb"));
    // this time the explicit request will cause a failure
    solver.add_request(request!({"var": "build-dep/=1.0.0"}));

    let res = run_and_print_resolve_for_tests(&solver).await;
    assert!(res.is_err());
}

#[rstest]
#[tokio::test]
async fn test_solver_option_compat_intersection(mut solver: Solver) {
    // A var option for spi-platform/~2022.4.1.4 should be able to resolve
    // with a build of openimageio that requires spi-platform/~2022.4.1.3.

    let spi_platform_1_0 = make_build!({"pkg": "spi-platform/1.0", "compat": "x.x.a.b"});
    let spi_platform_2022_4_1_3 =
        make_build!({"pkg": "spi-platform/2022.4.1.3", "compat": "x.x.a.b"});
    let spi_platform_2022_4_1_4 =
        make_build!({"pkg": "spi-platform/2022.4.1.4", "compat": "x.x.a.b"});
    let openimageio_1_2_3 = make_build!(
            {
                "pkg": "openimageio/1.2.3",
                "build": {
                    "options": [
                        { "pkg": "spi-platform/~2022.4.1.3" },
                    ],
                },
            },
            [spi_platform_2022_4_1_3]
    );

    let repo = make_repo!([
        spi_platform_1_0,
        spi_platform_2022_4_1_3,
        spi_platform_2022_4_1_4,
        openimageio_1_2_3,
    ]);

    solver.add_repository(Arc::new(repo));
    solver.add_request(request!({"var": "spi-platform/~2022.4.1.4"}));
    solver.add_request(request!({"pkg": "openimageio"}));

    let _ = run_and_print_resolve_for_tests(&solver).await.unwrap();
}

#[rstest]
#[tokio::test]
async fn test_solver_components(mut solver: Solver) {
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

    let solution = run_and_print_resolve_for_tests(&solver).await.unwrap();

    let resolved = solution
        .get("python")
        .unwrap()
        .request
        .pkg
        .components
        .clone();
    let expected = ["interpreter", "doc", "lib", "run"]
        .iter()
        .map(|c| Component::parse(c).map_err(|err| err.into()))
        .map(Result::unwrap)
        .collect();
    assert_eq!(resolved, expected);
}

#[rstest]
#[tokio::test]
async fn test_solver_components_when_no_components_requested(mut solver: Solver) {
    // test when a package is requested with no components and the
    // package is one that has components
    // - the default component(s) should be the ones in the resolve
    //   for that package
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
                    "requirements": [{"pkg": "python/3.7.3"}, {"pkg": "pkgb"}]
                },
            },
            {
                "pkg": "pkgb",
                "install": {"requirements": [{"pkg": "python"}]},
            },
        ]
    );

    solver.add_repository(Arc::new(repo));
    solver.add_request(request!("pkga"));
    solver.add_request(request!("pkgb"));

    let solution = run_and_print_resolve_for_tests(&solver).await.unwrap();

    let resolved = solution
        .get("python")
        .unwrap()
        .request
        .pkg
        .components
        .clone();
    let expected = [Component::default_for_run()]
        .iter()
        .map(|c| Component::parse(c).map_err(|err| err.into()))
        .map(Result::unwrap)
        .collect();
    assert_eq!(resolved, expected);
}

#[rstest]
#[tokio::test]
async fn test_solver_src_package_request_when_no_components_requested(mut solver: Solver) {
    // test when a /src package build is requested with no components
    // and a matching package with a /src package build exists in the repo
    // - the solver should resolve to the /src package build
    let repo = make_repo!(
        [
            {
                "pkg": "mypkg/1.2.3",
            },
            {
                "pkg": "mypkg/1.2.3/src",
            },
        ]
    );
    solver.add_repository(Arc::new(repo));

    let req = request!("mypkg/1.2.3/src");
    solver.add_request(req);

    let solution = run_and_print_resolve_for_tests(&solver).await.unwrap();
    let resolved = solution.get("mypkg").unwrap().spec.ident().clone();

    let expected = build_ident!("mypkg/1.2.3/src");
    assert_eq!(resolved, expected);
}

#[rstest]
#[tokio::test]
async fn test_solver_all_component(mut solver: Solver) {
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

    let solution = run_and_print_resolve_for_tests(&solver).await.unwrap();

    let resolved = solution.get("python").unwrap();
    assert_eq!(resolved.request.pkg.components.len(), 1);
    assert_eq!(
        resolved.request.pkg.components.iter().next(),
        Some(&Component::All)
    );
    assert_resolved!(
        solution,
        "python",
        components = ["bin", "build", "dev", "doc", "lib", "run"]
    );
}

#[rstest]
#[tokio::test]
async fn test_solver_component_availability(mut solver: Solver) {
    // test when a package is requested with some component
    // - all the specs components are selected in the resolve
    // - the final build has published layers for each component

    let spec373 = recipe!({
        "pkg": "python/3.7.3",
        "install": {
            "components": [
                {"name": "bin", "uses": ["lib"]},
                {"name": "lib"},
            ]
        },
    });
    let spec372 = recipe!({
        "pkg": "python/3.7.2",
        "install": {
            "components": [
                {"name": "bin", "uses": ["lib"]},
                {"name": "lib"},
            ]
        },
    });
    let spec371 = recipe!({
        "pkg": "python/3.7.1",
        "install": {
            "components": [
                {"name": "bin", "uses": ["lib"]},
                {"name": "lib"},
            ]
        },
    });

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
    repo.publish_recipe(&spec373).await.unwrap();
    repo.publish_recipe(&spec372).await.unwrap();
    repo.publish_recipe(&spec371).await.unwrap();

    solver.add_repository(Arc::new(repo));
    solver.add_request(request!("python:bin"));

    let solution = run_and_print_resolve_for_tests(&solver).await.unwrap();

    assert_resolved!(
        solution,
        "python",
        "3.7.1",
        "should resolve the only version with all the components we need actually published"
    );
    assert_resolved!(solution, "python", components = ["bin", "lib"]);
}

#[rstest]
#[tokio::test]
async fn test_solver_component_requirements(mut solver: Solver) {
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

    let solution = run_and_print_resolve_for_tests(&solver).await.unwrap();

    solution.get("dep").expect("should exist");
    solution.get("depb").expect("should exist");
    assert!(solution.get("depr").is_none());

    solver.reset();
    solver.add_repository(repo);
    solver.add_request(request!("mypkg:run"));

    let solution = run_and_print_resolve_for_tests(&solver).await.unwrap();

    solution.get("dep").expect("should exist");
    solution.get("depr").expect("should exist");
    assert!(solution.get("depb").is_none());
}

#[rstest]
#[tokio::test]
async fn test_solver_component_requirements_extending(mut solver: Solver) {
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

    let solution = run_and_print_resolve_for_tests(&solver).await.unwrap();

    solution.get("depc").expect("should exist");
}

#[rstest]
#[tokio::test]
async fn test_solver_component_embedded(mut solver: Solver) {
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

    let solution = run_and_print_resolve_for_tests(&solver).await.unwrap();

    assert_resolved!(
        solution,
        "dep-e1",
        build = Build::Embedded(EmbeddedSource::Unknown)
    );

    solver.reset();
    solver.add_repository(repo);
    solver.add_request(request!("downstream2"));

    // should fail because the one embedded package
    // does not meet the requirements in downstream spec

    let res = run_and_print_resolve_for_tests(&solver).await;

    assert!(res.is_err());
}

#[rstest]
fn test_solver_get_request_validator() {
    let solver = Solver::default();
    let resolve_validator = solver.request_validator();
    assert!(
        resolve_validator.num_possible_hits() == 0,
        "A new solver should return a new resolve validator"
    )
}

#[rstest]
#[tokio::test]
async fn test_request_default_component() {
    let mut solver = Solver::default();
    solver.add_request(request!("python/3.7.3"));
    let state = solver.get_initial_state();
    let request = state
        .get_pkg_requests()
        .iter()
        .next()
        .expect("solver should have a request");
    assert_eq!(
        request.pkg.components,
        vec![Component::default_for_run()].into_iter().collect(),
        "solver should inject a default run component if not otherwise given"
    )
}

#[rstest]
fn test_error_frequency() {
    let mut solver = Solver::default();

    let mut errors = solver.error_frequency();
    assert!(errors.is_empty());

    let an_error: String = "An error".to_string();
    solver.increment_error_count(ErrorDetails::Message(an_error.clone()));
    errors = solver.error_frequency();
    assert!(errors.len() == 1);

    solver.increment_error_count(ErrorDetails::Message(an_error.clone()));
    errors = solver.error_frequency();
    assert!(errors.len() == 1);

    match errors.get(&an_error) {
        Some(error_freq) => assert!(
            error_freq.counter == 2,
            " error frequency count for error was incorrect, it should be 2 not {}",
            error_freq.counter
        ),
        None => panic!("error frequency count for error was missing, should have been 2"),
    }
}

#[rstest]
fn test_error_frequency_get_message_for_string_error() {
    let mut solver = Solver::default();

    let an_error: String = "An error".to_string();
    solver.increment_error_count(ErrorDetails::Message(an_error.clone()));
    let errors = solver.error_frequency();

    match errors.get(&an_error) {
        Some(error_freq) => assert!(
            error_freq.get_message(an_error.clone()) == an_error,
            " error frequency get_message for a string error should be the same as the error key not: {}",
            an_error.clone()
        ),
        None => panic!("error frequency for an_error was missing"),
    }
}

#[rstest]
fn test_error_frequency_get_message_for_couldnotsatisfy_error() {
    let mut solver = Solver::default();

    let error = "my-pkg";
    let request = PkgRequest::new(parse_ident_range(error).unwrap(), RequestedBy::CommandLine);

    solver.increment_error_count(ErrorDetails::CouldNotSatisfy(
        request.pkg.to_string(),
        request.get_requesters(),
    ));
    let errors: &std::collections::HashMap<String, super::ErrorFreq> = solver.error_frequency();

    match errors.get(&request.pkg.to_string()) {
        Some(error_freq) => assert!(
            error_freq.get_message(request.pkg.to_string()) != request.pkg.to_string(),
            " error frequency get_message for a 'could not satisfy' error should be more than the error key not: {}",
            error_freq.get_message(request.pkg.to_string())
        ),
        None => panic!("error frequency for a 'could not satisfy' error was missing"),
    }
}

#[rstest]
fn test_error_frequency_get_message_for_couldnotsatisfy_error_multiple() {
    let mut solver = Solver::default();

    let error = "my-pkg";
    let request = PkgRequest::new(parse_ident_range(error).unwrap(), RequestedBy::CommandLine);

    solver.increment_error_count(ErrorDetails::CouldNotSatisfy(
        request.pkg.to_string(),
        request.get_requesters(),
    ));
    solver.increment_error_count(ErrorDetails::CouldNotSatisfy(
        request.pkg.to_string(),
        vec![RequestedBy::SpkInternalTest],
    ));
    let errors: &std::collections::HashMap<String, super::ErrorFreq> = solver.error_frequency();

    match errors.get(&request.pkg.to_string()) {
        Some(error_freq) => {
            assert!(error_freq.counter == 2, "error frequency counter for 2 occurances of the same 'could not satisfy' error should be 2, not: {}", error_freq.counter );
            assert!(error_freq.get_message(request.pkg.to_string()) != request.pkg.to_string(),
            " error frequency get_message for a 'could not satisfy' error should be more than the error key not: {}",
            error_freq.get_message(request.pkg.to_string())
            )
        }
        None => panic!("error frequency for a 'could not satisfy error' was missing"),
    }
}

#[rstest]
fn test_problem_packages() {
    let mut solver = Solver::default();

    let mut problems = solver.problem_packages();
    assert!(problems.is_empty());

    let a_package: String = "package".to_string();
    solver.increment_problem_package_count(a_package.clone());
    problems = solver.problem_packages();
    assert!(problems.len() == 1);

    solver.increment_problem_package_count(a_package.clone());
    problems = solver.problem_packages();
    assert!(problems.len() == 1);

    match problems.get(&a_package) {
        Some(count) => assert!(
            *count == 2,
            " problem package count was incorrect, it should be 2 not {count}"
        ),
        None => panic!("problem package count was missing, should have been 2"),
    };
}
