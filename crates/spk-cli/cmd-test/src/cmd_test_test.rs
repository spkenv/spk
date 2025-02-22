// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use clap::Parser;
use rstest::rstest;
use spk_cli_common::Run;
use spk_cmd_build::build_package;
use spk_schema::foundation::fixtures::*;
use spk_storage::fixtures::*;

use super::CmdTest;

#[derive(Parser)]
struct TestOpt {
    #[clap(flatten)]
    test: CmdTest,
}

#[rstest]
#[case::cli("cli")]
#[case::checks("checks")]
#[case::resolvo("resolvo")]
#[tokio::test]
async fn test_all_test_stages_succeed(tmpdir: tempfile::TempDir, #[case] solver_to_run: &str) {
    // A var that appears in the variant list and doesn't appear in the
    // build.options list should still affect the build hash / produce a
    // unique build.
    let _rt = spfs_runtime().await;

    let filename_str = build_package!(
        tmpdir,
        "simple.spk.yaml",
        br#"
pkg: simple/1.0.0
build:
  script:
    - "true"

tests:
  - stage: sources
    script:
      - "true"
  - stage: build
    script:
      - "true"
  - stage: install
    script:
      - "true"
"#,
        solver_to_run
    );

    let mut opt = TestOpt::try_parse_from([
        "test",
        // Don't exec a new process to move into a new runtime, this confuses
        // coverage testing.
        "--no-runtime",
        "--disable-repo=origin",
        filename_str,
    ])
    .unwrap();
    opt.test.run().await.unwrap();
}

#[rstest]
#[case::cli("cli")]
#[case::checks("checks")]
#[case::resolvo("resolvo")]
#[tokio::test]
async fn test_install_test_picks_same_digest_as_build(
    tmpdir: tempfile::TempDir,
    #[case] solver_to_run: &str,
) {
    let _rt = spfs_runtime().await;

    build_package!(
        tmpdir,
        "dep.spk.yaml",
        br#"
pkg: a-pkg-with-no-version-specified/1.0.0
build:
  script:
    - "true"
"#,
        solver_to_run
    );

    let filename_str = build_package!(
        tmpdir,
        "simple.spk.yaml",
        br#"
pkg: simple/1.0.0
build:
  options:
    - pkg: a-pkg-with-no-version-specified
  script:
    - "true"

tests:
  - stage: install
    script:
      - "true"
"#,
        solver_to_run
    );

    let mut opt = TestOpt::try_parse_from([
        "test",
        // Don't exec a new process to move into a new runtime, this confuses
        // coverage testing.
        "--no-runtime",
        "--disable-repo=origin",
        filename_str,
    ])
    .unwrap();

    // The test should be looking for the same build digest of "simple" that
    // the build of "simple" created.
    opt.test
        .run()
        .await
        .expect("spk test should not have a solver error");
}

#[rstest]
#[case::cli("cli")]
#[case::checks("checks")]
#[case::resolvo("resolvo")]
#[tokio::test]
async fn test_install_test_picks_same_digest_as_build_with_new_dep_in_variant(
    tmpdir: tempfile::TempDir,
    #[case] solver_to_run: &str,
) {
    let _rt = spfs_runtime().await;

    build_package!(
        tmpdir,
        "dep-a.spk.yaml",
        br#"
pkg: dep-a/1.2.3
build:
  script:
    - "true"
"#,
        solver_to_run
    );

    build_package!(
        tmpdir,
        "dep-b.spk.yaml",
        br#"
pkg: dep-b/1.2.3
build:
  script:
    - "true"
"#,
        solver_to_run
    );

    // Note that "dep-b" is introduced as a new dependency in the variant.
    let filename_str = build_package!(
        tmpdir,
        "simple.spk.yaml",
        br#"
pkg: simple/1.0.0
build:
  options:
    - pkg: dep-a/1.2.3
  variants:
    - { dep-b: 1.2.3 }
  script:
    - "true"

tests:
  - stage: install
    script:
      - "true"
"#,
        solver_to_run
    );

    let mut opt = TestOpt::try_parse_from([
        "test",
        // Don't exec a new process to move into a new runtime, this confuses
        // coverage testing.
        "--no-runtime",
        "--disable-repo=origin",
        filename_str,
    ])
    .unwrap();

    // The test should be looking for the same build digest of "simple" that
    // the build of "simple" created.
    opt.test
        .run()
        .await
        .expect("spk test should not have a solver error");
}

#[rstest]
#[case::cli("cli")]
#[case::checks("checks")]
#[case::resolvo("resolvo")]
#[tokio::test]
async fn test_install_test_picks_same_digest_as_build_with_new_dep_in_variant_plus_command_line_overrides(
    tmpdir: tempfile::TempDir,
    #[case] solver_to_run: &str,
) {
    let _rt = spfs_runtime().await;

    build_package!(
        tmpdir,
        "dep-a.spk.yaml",
        br#"
pkg: dep-a/1.2.5
build:
  script:
    - "true"
"#,
        solver_to_run
    );

    build_package!(
        tmpdir,
        "dep-b.spk.yaml",
        br#"
pkg: dep-b/1.2.3
build:
  script:
    - "true"
"#,
        solver_to_run
    );

    let filename_str = build_package!(
        tmpdir,
        "simple.spk.yaml",
        br#"
pkg: simple/1.0.0
build:
  options:
    - pkg: dep-a/1.2.3
  variants:
    - { dep-b: 1.2.3 }
  script:
    - "true"

tests:
  - stage: install
    script:
      - "true"
"#,
        solver_to_run,
        // Extra build options specified here.
        "--opt",
        "dep-a=1.2.4"
    );

    let mut opt = TestOpt::try_parse_from([
        "test",
        // Don't exec a new process to move into a new runtime, this confuses
        // coverage testing.
        "--no-runtime",
        "--disable-repo=origin",
        // Add a command line override.
        "--opt",
        "dep-a=1.2.4",
        filename_str,
    ])
    .unwrap();

    // The test should be looking for the same build digest of "simple" that
    // the build of "simple" created.
    opt.test
        .run()
        .await
        .expect("spk test should not have a solver error");
}

#[rstest]
#[case::cli("cli")]
#[case::checks("checks")]
#[case::resolvo("resolvo")]
#[tokio::test]
async fn test_install_test_picks_same_digest_as_build_with_circular_dependencies(
    tmpdir: tempfile::TempDir,
    #[case] solver_to_run: &str,
) {
    let _rt = spfs_runtime().await;

    // A common dependency.
    build_package!(
        tmpdir,
        "some-other.spk.yaml",
        br#"
pkg: some-other/1.2.0
build:
  script:
    - "true"
"#,
        solver_to_run
    );

    build_package!(
        tmpdir,
        "dep-a.spk.yaml",
        br#"
pkg: dep-a/1.2.0
build:
  options:
    - pkg: some-other
  script:
    - "true"
install:
  requirements:
    - pkg: some-other
      fromBuildEnv: true
"#,
        solver_to_run
    );

    build_package!(
        tmpdir,
        "dep-b.spk.yaml",
        br#"
pkg: dep-b/1.2.0
build:
  options:
    - pkg: dep-a
    - pkg: some-other
  script:
    - "true"
install:
  requirements:
    - pkg: dep-a
      fromBuildEnv: true
    - pkg: some-other
      fromBuildEnv: true
"#,
        solver_to_run
    );

    let filename_str = build_package!(
        tmpdir,
        "dep-a.spk.yaml",
        br#"
pkg: dep-a/1.2.1
build:
  options:
    - pkg: dep-b
    - pkg: some-other
  script:
    - "true"
  validation:
    rules:
      - allow: RecursiveBuild
install:
  requirements:
    - pkg: dep-b
      fromBuildEnv: true
    - pkg: some-other
      fromBuildEnv: true
tests:
  - stage: install
    script:
      - "true"
"#,
        solver_to_run
    );

    let mut opt = TestOpt::try_parse_from([
        "test",
        // Don't exec a new process to move into a new runtime, this confuses
        // coverage testing.
        "--no-runtime",
        "--disable-repo=origin",
        filename_str,
    ])
    .unwrap();

    // The test should be looking for the same build digest of "dep-a" that
    // the second build of "dep-a" created.
    opt.test
        .run()
        .await
        .expect("spk test should not have a solver error");
}

#[rstest]
#[case::cli("cli")]
#[case::checks("checks")]
#[case::resolvo("resolvo")]
#[tokio::test]
async fn test_selectors_with_component_names_match_correctly(
    tmpdir: tempfile::TempDir,
    #[case] solver_to_run: &str,
) {
    let _rt = spfs_runtime().await;

    let _ = build_package!(
        tmpdir,
        "base.spk.yaml",
        br#"
pkg: base/1.0.0
build:
  script:
    - touch "$PREFIX"/comp1
    - touch "$PREFIX"/comp2

install:
  components:
    - name: comp1
      files:
        - comp1
    - name: comp2
      files:
        - comp2
"#,
        solver_to_run
    );

    // This package is expected to pass both tests.

    let filename_str = build_package!(
        tmpdir,
        "simple1.spk.yaml",
        br#"
pkg: simple1/1.0.0
build:
  script:
    - "true"
  variants:
    - { "base:comp1": "1.0.0" }
    - { "base:comp2": "1.0.0" }

tests:
  - stage: build
    selectors:
      - { "base:comp1": "1.0.0" }
    script:
      - test -f "$PREFIX"/comp1
  - stage: build
    selectors:
      - { "base:comp2": "1.0.0" }
    script:
      - test -f "$PREFIX"/comp2
"#,
        solver_to_run
    );

    let mut opt = TestOpt::try_parse_from([
        "test",
        // Don't exec a new process to move into a new runtime, this confuses
        // coverage testing.
        "--no-runtime",
        "--disable-repo=origin",
        filename_str,
    ])
    .unwrap();
    opt.test.run().await.unwrap();

    // The above test would also pass if all the tests were skipped, so the
    // next tests verify that the tests are actually run.

    let filename_str = build_package!(
        tmpdir,
        "simple2.spk.yaml",
        br#"
pkg: simple2/1.0.0
build:
  script:
    - "true"
  variants:
    - { "base:comp1": "1.0.0" }
    - { "base:comp2": "1.0.0" }

tests:
  - stage: build
    selectors:
      - { "base:comp1": "1.0.0" }
    script:
      # Comp2 is expected to not exist and make this test fail.
      # If the whole test run fails we know that this selector matched as
      # expected.
      - test -f "$PREFIX"/comp2
"#,
        solver_to_run
    );

    let mut opt = TestOpt::try_parse_from([
        "test",
        // Don't exec a new process to move into a new runtime, this confuses
        // coverage testing.
        "--no-runtime",
        "--disable-repo=origin",
        filename_str,
    ])
    .unwrap();
    opt.test
        .run()
        .await
        .expect_err("the test run should fail, otherwise the selectors aren't working properly");

    let filename_str = build_package!(
        tmpdir,
        "simple3.spk.yaml",
        br#"
pkg: simple3/1.0.0
build:
  script:
    - "true"
  variants:
    - { "base:comp1": "1.0.0" }
    - { "base:comp2": "1.0.0" }

tests:
  - stage: build
    selectors:
      - { "base:comp2": "1.0.0" }
    script:
      # Comp1 is expected to not exist and make this test fail.
      # If the whole test run fails we know that this selector matched as
      # expected.
      - test -f "$PREFIX"/comp1
"#,
        solver_to_run
    );

    let mut opt = TestOpt::try_parse_from([
        "test",
        // Don't exec a new process to move into a new runtime, this confuses
        // coverage testing.
        "--no-runtime",
        "--disable-repo=origin",
        filename_str,
    ])
    .unwrap();
    opt.test
        .run()
        .await
        .expect_err("the test run should fail, otherwise the selectors aren't working properly");
}
