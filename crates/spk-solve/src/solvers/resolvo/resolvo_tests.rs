// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::borrow::Cow;

use rstest::rstest;
use spk_schema::prelude::HasVersion;
use spk_schema::{Package, opt_name};
use spk_solve_macros::{make_repo, request};

use super::Solver;

#[rstest]
#[tokio::test]
async fn basic() {
    let repo = make_repo!(
        [
            {"pkg": "basic/1.0.0"},
        ]
    );

    let mut solver = Solver::new(vec![repo.into()], Cow::Borrowed(&[]));
    let solution = solver.solve(&[request!("basic")]).await.unwrap();
    assert_eq!(solution.len(), 1);
}

#[rstest]
#[tokio::test]
async fn two_choices() {
    let repo = make_repo!(
        [
            {"pkg": "basic/2.0.0"},
            {"pkg": "basic/1.0.0"},
        ]
    );

    let mut solver = Solver::new(vec![repo.into()], Cow::Borrowed(&[]));
    let solution = solver.solve(&[request!("basic")]).await.unwrap();
    assert_eq!(solution.len(), 1);
    // All things being equal it should pick the higher version
    assert_eq!(
        solution.items().next().unwrap().spec.version().to_string(),
        "2.0.0"
    );
}

#[rstest]
#[tokio::test]
async fn two_choices_request_lower() {
    let repo = make_repo!(
        [
            {"pkg": "basic/2.0.0"},
            {"pkg": "basic/1.0.0"},
        ]
    );

    let mut solver = Solver::new(vec![repo.into()], Cow::Borrowed(&[]));
    let solution = solver.solve(&[request!("basic/1.0.0")]).await.unwrap();
    assert_eq!(solution.len(), 1);
    assert_eq!(
        solution.items().next().unwrap().spec.version().to_string(),
        "1.0.0"
    );
}

#[rstest]
#[tokio::test]
async fn two_choices_request_missing() {
    let repo = make_repo!(
        [
            {"pkg": "basic/3.0.0"},
            {"pkg": "basic/2.0.0"},
        ]
    );

    let mut solver = Solver::new(vec![repo.into()], Cow::Borrowed(&[]));
    let _solution = solver
        .solve(&[request!("basic/1.0.0")])
        .await
        .expect_err("Nothing satisfies 1.0.0");
}

#[rstest]
#[tokio::test]
async fn package_with_dependency() {
    let repo = make_repo!(
        [
            {"pkg": "dep/1.0.0"},
            {"pkg": "needs-dep/1.0.0",
             "install": {
                 "requirements": [
                     {"pkg": "dep"}
                 ]
             }
            },
        ]
    );

    let mut solver = Solver::new(vec![repo.into()], Cow::Borrowed(&[]));
    let solution = solver.solve(&[request!("needs-dep/1.0.0")]).await.unwrap();
    assert_eq!(solution.len(), 2);
}

#[rstest]
#[case::expect_blue("dep.color/blue", "blue")]
#[case::expect_red("dep.color/red", "red")]
#[should_panic]
#[case::expect_green("dep.color/green", "green")]
#[tokio::test]
async fn package_with_dependency_on_variant(
    #[case] color_spec: &str,
    #[case] expected_color: &str,
) {
    let repo = make_repo!(
        [
            {"pkg": "dep/1.0.0",
             "build": {
                 "options": [
                     {"var": "color/blue"}
                 ]
             }
            },
            {"pkg": "dep/1.0.0",
             "build": {
                 "options": [
                     {"var": "color/red"}
                 ]
             }
            },
            {"pkg": "needs-dep/1.0.0",
             "install": {
                 "requirements": [
                     {"pkg": "dep"},
                     {"var": color_spec},
                 ]
             }
            },
        ]
    );

    let mut solver = Solver::new(vec![repo.into()], Cow::Borrowed(&[]));
    let solution = solver.solve(&[request!("needs-dep/1.0.0")]).await.unwrap();
    assert_eq!(solution.len(), 2);
    let dep = solution.get("dep").unwrap();
    assert_eq!(
        dep.spec.option_values().get(opt_name!("color")).unwrap(),
        expected_color
    );
}

#[rstest]
#[case::expect_blue("color/blue", "blue")]
#[case::expect_red("color/red", "red")]
#[should_panic]
#[case::expect_green("color/green", "green")]
#[tokio::test]
async fn global_vars(#[case] global_spec: &str, #[case] expected_color: &str) {
    let repo = make_repo!(
        [
            {"pkg": "dep/1.0.0",
             "build": {
                 "options": [
                     {"var": "color/blue"}
                 ]
             }
            },
            {"pkg": "dep/1.0.0",
             "build": {
                 "options": [
                     {"var": "color/red"}
                 ]
             }
            },
            {"pkg": "needs-dep/1.0.0",
             "install": {
                 "requirements": [
                     {"pkg": "dep"},
                     {"var": global_spec},
                 ]
             }
            },
        ]
    );

    let mut solver = Solver::new(vec![repo.into()], Cow::Borrowed(&[]));
    let solution = solver.solve(&[request!("needs-dep/1.0.0")]).await.unwrap();
    assert_eq!(solution.len(), 2);
    let dep = solution.get("dep").unwrap();
    assert_eq!(
        dep.spec.option_values().get(opt_name!("color")).unwrap(),
        expected_color
    );
}
