// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::fs::File;
use std::io::Write;

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
#[tokio::test]
async fn test_all_test_stages_succeed(tmpdir: tempfile::TempDir) {
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
"#
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
#[tokio::test]
async fn test_install_test_picks_same_digest_as_build(tmpdir: tempfile::TempDir) {
    let _rt = spfs_runtime().await;

    build_package!(
        tmpdir,
        "dep.spk.yaml",
        br#"
pkg: a-pkg-with-no-version-specified/1.0.0
build:
  script:
    - "true"
"#
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
"#
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
#[tokio::test]
async fn test_install_test_picks_same_digest_as_build_with_new_dep_in_variant(
    tmpdir: tempfile::TempDir,
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
"#
    );

    build_package!(
        tmpdir,
        "dep-b.spk.yaml",
        br#"
pkg: dep-b/1.2.3
build:
  script:
    - "true"
"#
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
"#
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
#[tokio::test]
async fn test_install_test_picks_same_digest_as_build_with_new_dep_in_variant_plus_command_line_overrides(
    tmpdir: tempfile::TempDir,
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
"#
    );

    build_package!(
        tmpdir,
        "dep-b.spk.yaml",
        br#"
pkg: dep-b/1.2.3
build:
  script:
    - "true"
"#
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
#[tokio::test]
async fn test_install_test_picks_same_digest_as_build_with_circular_dependencies(
    tmpdir: tempfile::TempDir,
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
"#
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
"#
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
"#
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
        "--allow-circular-dependencies",
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

    // The test should be looking for the same build digest of "dep-a" that
    // the second build of "dep-a" created.
    opt.test
        .run()
        .await
        .expect("spk test should not have a solver error");
}
