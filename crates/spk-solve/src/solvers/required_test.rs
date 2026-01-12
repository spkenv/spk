// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

//! Tests specific to the handling of required vars.

use std::sync::Arc;

use rstest::rstest;
use spk_schema::foundation::version_ident;
use spk_schema::ident::{
    PkgRequest,
    PkgRequestOptionValue,
    PkgRequestOptions,
    PkgRequestWithOptions,
    RequestWithOptions,
    RequestedBy,
    VarRequest,
    parse_ident_range,
};
use spk_schema::{Package, opt_name};
use spk_solve_macros::{make_build, make_repo, pinned_request};

use super::solver_test::{resolvo_solver, run_and_print_resolve_for_tests, step_solver};
use crate::solver::{SolverExt, SolverImpl, SolverMut};

/// A var defined as required means a build is not eligible unless the request
/// includes the var.
#[rstest]
#[case::step(step_solver())]
#[case::resolvo(resolvo_solver())]
#[tokio::test]
async fn required_var_blocks_being_picked(#[case] mut solver: SolverImpl) {
    let repo = make_repo!(
        [
            {
                "pkg": "mylib/1.0.0",
                "build": {
                    "options": [
                        {
                            "var": "namespace_style/major_minor",
                            "required": true,
                        },
                    ]
                }
            },
            {
                "pkg": "mypkg/1.0.0",
                "install": {
                    "requirements": [{"pkg": "mylib"}],
                }
            },
        ]
    );
    let repo = Arc::new(repo);

    solver.add_repository(repo);
    solver.add_request(pinned_request!("mypkg"));

    let _solution = run_and_print_resolve_for_tests(&mut solver)
        .await
        .expect_err("mypkg expected to not solve");
}

/// Specifying a non-namespaced var does not match a required var
#[rstest]
#[case::step(step_solver())]
#[case::resolvo(resolvo_solver())]
#[tokio::test]
async fn global_var_not_valid_for_required(#[case] mut solver: SolverImpl) {
    let repo = make_repo!(
        [
            {
                "pkg": "mylib/1.0.0",
                "build": {
                    "options": [
                        {
                            "var": "namespace_style/major_minor",
                            "required": true,
                        },
                    ]
                }
            },
            {
                "pkg": "mypkg/1.0.0",
                "build": {
                    "options": [
                        {
                            "var": "namespace_style/major_minor",
                        },
                    ]
                },
                "install": {
                    "requirements": [{"pkg": "mylib"}],
                }
            },
        ]
    );
    let repo = Arc::new(repo);

    solver.add_repository(repo);
    solver.add_request(pinned_request!("mypkg"));

    let _solution = run_and_print_resolve_for_tests(&mut solver)
        .await
        .expect_err("mypkg expected to not solve");
}

/// Request including the required var allows the package to be picked
#[rstest]
#[case::step(step_solver())]
#[case::resolvo(resolvo_solver())]
#[tokio::test]
async fn requested_required_var_allows_being_picked(#[case] mut solver: SolverImpl) {
    let repo = make_repo!(
        [
            {
                "pkg": "mylib/1.0.0",
                "build": {
                    "options": [
                        {
                            "var": "namespace_style/major_minor",
                            "required": true,
                        },
                    ]
                }
            },
            {
                "pkg": "mypkg/1.0.0",
                "build": {
                    "options": [
                        {
                            "var": "mylib.namespace_style/major_minor",
                        },
                    ]
                },
                "install": {
                    "requirements": [{"pkg": "mylib"}],
                }
            },
        ]
    );
    let repo = Arc::new(repo);

    solver.add_repository(repo);
    solver.add_request(pinned_request!("mypkg"));

    let _solution = run_and_print_resolve_for_tests(&mut solver)
        .await
        .expect("mypkg expected to solve");
}

/// A top-level pkg request doesn't satisfy a required var in a dependency
#[rstest]
#[case::step(step_solver())]
#[case::resolvo(resolvo_solver())]
#[tokio::test]
async fn top_level_pkg_request_not_enough(#[case] mut solver: SolverImpl) {
    let repo = make_repo!(
        [
            {
                "pkg": "mylib/1.0.0",
                "build": {
                    "options": [
                        {
                            "var": "namespace_style/major_minor",
                            "required": true,
                        },
                    ]
                }
            },
            {
                "pkg": "mypkg/1.0.0",
                "install": {
                    "requirements": [{"pkg": "mylib"}],
                }
            },
        ]
    );
    let repo = Arc::new(repo);

    solver.add_repository(repo);
    // Adding this request doesn't change the fact that "mypkg" was (presumably)
    // built with a build of "mylib" that doesn't have the required var set.
    solver.add_request(RequestWithOptions::Pkg(PkgRequestWithOptions {
        pkg_request: PkgRequest::new(
            parse_ident_range("mylib").unwrap(),
            RequestedBy::SpkInternalTest,
        ),
        options: PkgRequestOptions::from_iter(vec![(
            opt_name!("mylib.namespace_style").into(),
            PkgRequestOptionValue::Complete("major_minor".into()),
        )]),
    }));
    solver.add_request(pinned_request!("mypkg"));

    let _solution = run_and_print_resolve_for_tests(&mut solver)
        .await
        .expect_err("mypkg expected to not solve");
}

/// A top-level var request doesn't satisfy a required var in a dependency
#[rstest]
#[case::step(step_solver())]
#[case::resolvo(resolvo_solver())]
#[tokio::test]
async fn top_level_var_request_not_enough(#[case] mut solver: SolverImpl) {
    let repo = make_repo!(
        [
            {
                "pkg": "mylib/1.0.0",
                "build": {
                    "options": [
                        {
                            "var": "namespace_style/major_minor",
                            "required": true,
                        },
                    ]
                }
            },
            {
                "pkg": "mypkg/1.0.0",
                "install": {
                    "requirements": [{"pkg": "mylib"}],
                }
            },
        ]
    );
    let repo = Arc::new(repo);

    solver.add_repository(repo);
    // Adding this request doesn't change the fact that "mypkg" was (presumably)
    // built with a build of "mylib" that doesn't have the required var set.
    solver.add_request(RequestWithOptions::Var(VarRequest {
        var: opt_name!("mylib.namespace_style").into(),
        value: "major_minor".into(),
        description: None,
    }));
    solver.add_request(pinned_request!("mypkg"));

    let _solution = run_and_print_resolve_for_tests(&mut solver)
        .await
        .expect_err("mypkg expected to not solve");
}

/// "Required" vars need to have strong inheritance behavior to work properly.
#[rstest]
#[tokio::test]
async fn requested_required_var_is_strongly_inherited() {
    let mylib = make_build!(
        {
            "pkg": "mylib/1.0.0",
            "build": {
                "options": [
                    {
                        "var": "namespace_style/major_minor",
                        "required": true,
                    },
                ]
            }
        }
    );

    let mypkg = make_build!(
            {
                "pkg": "mypkg/1.0.0",
                "build": {
                    "options": [
                        {
                            "pkg": "mylib",
                        },
                        {
                            "var": "mylib.namespace_style/major_minor",
                        },
                    ]
                },
                "install": {
                    "requirements":
                        [
                            // this test should *not* explicitly request the
                            // required var from mylib; verify that it is
                            // inherited as expected.
                            {"pkg": "mylib"}
                        ],
                }
            },
            [mylib]
    );

    let repo = make_repo!([mylib, mypkg]);
    let repo = Arc::new(repo);
    let builds = repo
        .list_package_builds(&version_ident!("mypkg/1.0.0"))
        .await
        .unwrap();
    let mypkg = repo.read_package(&builds[0]).await.unwrap();
    mypkg
        .runtime_requirements_with_options()
        .iter()
        .find(|req| {
            let RequestWithOptions::Var(var_req) = req else {
                return false;
            };
            var_req.var.as_str() == "mylib.namespace_style" && &*var_req.value == "major_minor"
        })
        .expect("expected mypkg to have inherited required var from mylib");
}
