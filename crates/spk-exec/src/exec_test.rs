// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::collections::HashMap;
use std::sync::Arc;

use rstest::{fixture, rstest};
use spk_cmd_build::build_package;
use spk_schema::foundation::build_ident;
use spk_schema::foundation::fixtures::*;
use spk_solve::{DecisionFormatterBuilder, SolverExt, SolverMut, StepSolver};
use spk_solve_macros::request;
use spk_storage::fixtures::*;

use crate::solution_to_resolved_runtime_layers;

#[fixture]
fn solver() -> StepSolver {
    StepSolver::default()
}

/// If two layers contribute files to the same subdirectory, the Manifest is
/// expected to contain both files.
#[rstest]
#[case::cli("cli")]
#[case::checks("checks")]
#[case::resolvo("resolvo")]
#[tokio::test]
async fn get_environment_filesystem_merges_directories(
    tmpdir: tempfile::TempDir,
    // TODO: test with all solvers
    mut solver: StepSolver,
    #[case] solver_to_run: &str,
) {
    let rt = spfs_runtime().await;

    build_package!(
        tmpdir,
        "one.spk.yaml",
        br#"
api: v0/package
pkg: one/1.0.0

build:
  script:
    - mkdir "$PREFIX"/subdir
    - touch "$PREFIX"/subdir/one.txt
"#,
        solver_to_run
    );

    build_package!(
        tmpdir,
        "two.spk.yaml",
        br#"
api: v0/package
pkg: two/1.0.0

build:
  script:
    - mkdir "$PREFIX"/subdir
    - touch "$PREFIX"/subdir/two.txt
"#,
        solver_to_run
    );

    let formatter = DecisionFormatterBuilder::default()
        .with_verbosity(0)
        .build();

    solver.add_repository(Arc::clone(&rt.tmprepo));
    solver.add_request(request!("one"));
    solver.add_request(request!("two"));

    let solution = solver.run_and_log_resolve(&formatter).await.unwrap();

    let resolved_layers = solution_to_resolved_runtime_layers(&solution).unwrap();

    let mut conflicting_packages = HashMap::new();
    let environment = resolved_layers
        .get_environment_filesystem(
            build_ident!("does-not-matter/1.0.0/src"),
            &mut conflicting_packages,
        )
        .await
        .unwrap();

    assert!(environment.get_path("subdir/one.txt").is_some());
    assert!(environment.get_path("subdir/two.txt").is_some());
}
