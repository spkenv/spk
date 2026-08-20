// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::borrow::Cow;

use rstest::rstest;
use spk_schema::prelude::HasVersion;
use spk_schema::{OptionValues, Package, opt_name};
use spk_solve_macros::{make_repo, pinned_request};
use tap::TapFallible;

use super::Solver;
use crate::SolverMut;

#[rstest]
#[tokio::test]
async fn basic() {
    let repo = make_repo!(
        [
            {"pkg": "basic/1.0.0"},
        ]
    );

    let mut solver = Solver::new(vec![repo.into()], Cow::Borrowed(&[]));
    solver.add_request(pinned_request!("basic"));
    let solution = solver.solve().await.unwrap();
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
    solver.add_request(pinned_request!("basic"));
    let solution = solver.solve().await.unwrap();
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
    solver.add_request(pinned_request!("basic/1.0.0"));
    let solution = solver.solve().await.unwrap();
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
    solver.add_request(pinned_request!("basic/1.0.0"));
    let _solution = solver.solve().await.expect_err("Nothing satisfies 1.0.0");
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
    solver.add_request(pinned_request!("needs-dep/1.0.0"));
    let solution = solver.solve().await.tap_err(|e| eprintln!("{e}")).unwrap();
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
    solver.add_request(pinned_request!("needs-dep/1.0.0"));
    let solution = solver.solve().await.unwrap();
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
    solver.add_request(pinned_request!("needs-dep/1.0.0"));
    let solution = solver.solve().await.unwrap();
    assert_eq!(solution.len(), 2);
    let dep = solution.get("dep").unwrap();
    assert_eq!(
        dep.spec.option_values().get(opt_name!("color")).unwrap(),
        expected_color
    );
}

#[rstest]
#[tokio::test]
async fn package_with_source_build() {
    let repo = make_repo!(
        [
            {"pkg": "dep/1.0.0/src"},
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
    solver.add_request(pinned_request!("needs-dep/1.0.0"));
    solver
        .solve()
        .await
        .expect_err("src build should not satisfy dependency");
}

/// An "ambient" request -- one from the command line or an ordinary
/// dependency, that does not name a specific build -- should not be satisfied
/// by an embedded stub while a real build is available. Resolving the stub
/// would pull the stub's parent into the solution as a side effect of a
/// request that never mentioned it.
#[rstest]
#[tokio::test]
async fn ambient_request_does_not_pull_in_embed_parent() {
    let repo = make_repo!(
        [
            {"pkg": "qt/5.12.6"},
            // Embeds a *higher* version of qt than the real build.
            {
                "pkg": "maya/2019.2",
                "install": {"embedded": [{"pkg": "qt/5.13.0"}]},
            },
        ]
    );

    let mut solver = Solver::new(vec![repo.into()], Cow::Borrowed(&[]));
    solver.add_request(pinned_request!("qt"));
    let solution = solver.solve().await.unwrap();

    let qt = solution.get("qt").expect("qt is in the solution");
    assert!(
        !qt.spec.ident().build().is_embedded(),
        "expected the real build of qt, got {}",
        qt.spec.ident()
    );
    assert!(
        solution.get("maya").is_none(),
        "requesting qt must not drag in the package that embeds it"
    );
}

/// Given a choice between a build that embeds a package and a lower-versioned
/// build that does not, prefer the one that keeps the solution stub-free.
#[rstest]
#[tokio::test]
async fn ambient_request_prefers_embed_free_solution() {
    let repo = make_repo!(
        [
            {"pkg": "qt/5.12.6"},
            // Higher version, embeds qt.
            {
                "pkg": "maya/2019.2",
                "install": {"embedded": [{"pkg": "qt/5.12.6"}]},
            },
            // Lower version, uses the real qt.
            {
                "pkg": "maya/2019.0",
                "install": {"requirements": [{"pkg": "qt/5.12.6"}]},
            },
            {
                "pkg": "app/1.0.0",
                "install": {"requirements": [{"pkg": "maya"}, {"pkg": "qt"}]},
            },
        ]
    );

    let mut solver = Solver::new(vec![repo.into()], Cow::Borrowed(&[]));
    solver.add_request(pinned_request!("app"));
    let solution = solver.solve().await.unwrap();

    let qt = solution.get("qt").expect("qt is in the solution");
    assert!(
        !qt.spec.ident().build().is_embedded(),
        "expected the real build of qt, got {}",
        qt.spec.ident()
    );
    assert_eq!(
        solution
            .get("maya")
            .expect("maya is in the solution")
            .spec
            .version()
            .to_string(),
        "2019.0.0",
        "expected the maya build that does not embed qt"
    );
}

/// The restriction above is only a preference. When no stub-free solution
/// exists the solve is retried without it, and the stub -- along with the
/// parent that provides it -- is used after all.
#[rstest]
#[tokio::test]
async fn ambient_request_falls_back_to_stub_when_only_option() {
    let repo = make_repo!(
        [
            // The only source of qt is the one maya embeds.
            {
                "pkg": "maya/2019.2",
                "install": {"embedded": [{"pkg": "qt/5.12.6"}]},
            },
        ]
    );

    let mut solver = Solver::new(vec![repo.into()], Cow::Borrowed(&[]));
    solver.add_request(pinned_request!("qt"));
    let solution = solver.solve().await.unwrap();

    let qt = solution.get("qt").expect("qt is in the solution");
    assert!(
        qt.spec.ident().build().is_embedded(),
        "expected the embedded stub, got {}",
        qt.spec.ident()
    );
    assert!(
        solution.get("maya").is_some(),
        "the parent providing the stub must be in the solution"
    );
}

/// A stub resolved because its parent is already in the solution is not
/// affected: the parent's `install.embedded` names the stub's build
/// explicitly, so the restriction does not apply to it.
#[rstest]
#[tokio::test]
async fn parent_in_solution_still_resolves_its_stub() {
    let repo = make_repo!(
        [
            {"pkg": "qt/5.12.6"},
            {
                "pkg": "maya/2019.2",
                "install": {"embedded": [{"pkg": "qt/5.12.6"}]},
            },
        ]
    );

    let mut solver = Solver::new(vec![repo.into()], Cow::Borrowed(&[]));
    // Only maya is requested; nothing asks for qt on its own.
    solver.add_request(pinned_request!("maya"));
    let solution = solver.solve().await.unwrap();

    let qt = solution
        .get("qt")
        .expect("maya's embedded qt is in the solution");
    assert!(
        qt.spec.ident().build().is_embedded(),
        "expected maya's embedded stub, got {}",
        qt.spec.ident()
    );
}

/// Allowing a stub that is the only way to satisfy one request must not
/// relax the restriction for unrelated requests. Here `qt` exists only inside
/// `maya`, so its stub has to be allowed; `png` has a real build, so the
/// solution should still take the `imaging` build that does not embed it.
#[rstest]
#[tokio::test]
async fn allowing_a_required_stub_does_not_permit_unrelated_ones() {
    let repo = make_repo!(
        [
            // The only source of qt is the one maya embeds.
            {
                "pkg": "maya/2019.2",
                "install": {"embedded": [{"pkg": "qt/5.12.6"}]},
            },
            {"pkg": "png/1.6.0"},
            // Higher version, embeds png.
            {
                "pkg": "imaging/2.0.0",
                "install": {"embedded": [{"pkg": "png/1.6.0"}]},
            },
            // Lower version, uses the real png.
            {
                "pkg": "imaging/1.0.0",
                "install": {"requirements": [{"pkg": "png/1.6.0"}]},
            },
            {
                "pkg": "app/1.0.0",
                "install": {
                    "requirements": [{"pkg": "qt"}, {"pkg": "imaging"}, {"pkg": "png"}]
                },
            },
        ]
    );

    let mut solver = Solver::new(vec![repo.into()], Cow::Borrowed(&[]));
    solver.add_request(pinned_request!("app"));
    let solution = solver.solve().await.unwrap();

    // qt had no alternative, so its stub is used and maya comes with it.
    let qt = solution.get("qt").expect("qt is in the solution");
    assert!(
        qt.spec.ident().build().is_embedded(),
        "expected maya's embedded qt, got {}",
        qt.spec.ident()
    );
    assert!(solution.get("maya").is_some(), "maya provides the qt stub");

    // png did have an alternative, so it must not have been relaxed too.
    let png = solution.get("png").expect("png is in the solution");
    assert!(
        !png.spec.ident().build().is_embedded(),
        "allowing qt's stub must not permit png's, got {}",
        png.spec.ident()
    );
    assert_eq!(
        solution
            .get("imaging")
            .expect("imaging is in the solution")
            .spec
            .version()
            .to_string(),
        "1.0.0",
        "expected the imaging build that does not embed png"
    );
}
