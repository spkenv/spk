// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::collections::HashMap;
use std::sync::Arc;

use rstest::{fixture, rstest};
use spk_cmd_build::build_package;
use spk_schema::foundation::fixtures::*;
use spk_schema::ident::build_ident;
use spk_solve::{DecisionFormatterBuilder, StepSolver};
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
#[tokio::test]
async fn get_environment_filesystem_merges_directories(
    tmpdir: tempfile::TempDir,
    mut solver: StepSolver,
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
    );

    let formatter = DecisionFormatterBuilder::default()
        .with_verbosity(0)
        .build();

    solver.add_repository(Arc::clone(&rt.tmprepo));
    solver.add_request(request!("one"));
    solver.add_request(request!("two"));

    let (solution, _) = formatter.run_and_log_resolve(&solver).await.unwrap();

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
