// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::fs::File;
use std::io::Write;

use clap::Parser;
use rstest::rstest;
use spk_cli_common::Run;
use spk_schema::foundation::fixtures::*;
use spk_schema::ident::version_ident;
use spk_storage::fixtures::*;

use super::Build;
use crate::{build_package, try_build_package};

#[derive(Parser)]
struct Opt {
    #[clap(flatten)]
    build: Build,
}

#[rstest]
#[tokio::test]
async fn test_variant_options_contribute_to_build_hash(tmpdir: tempfile::TempDir) {
    // A var that appears in the variant list and doesn't appear in the
    // build.options list should still affect the build hash / produce a
    // unique build.
    let rt = spfs_runtime().await;

    build_package!(
        tmpdir,
        "three-variants.spk.yaml",
        br#"
pkg: three-variants/1.0.0

build:
  variants:
    - { python.abi: cp27mu }
    - { python.abi: cp37m }
    - { python.abi: cp39 }
  script:
    - "true"
"#,
    );

    let ident = version_ident!("three-variants/1.0.0");

    let non_src_builds = rt
        .tmprepo
        .list_package_builds(&ident)
        .await
        .unwrap()
        .into_iter()
        .filter(|b| !b.is_source());

    assert_eq!(non_src_builds.count(), 3, "Expected three distinct builds");
}

#[rstest]
#[tokio::test]
async fn test_build_hash_not_affected_by_dependency_version(tmpdir: tempfile::TempDir) {
    // The same recipe should produce the same build hash even if there is a
    // change in its dependencies (at resolve time).
    let rt = spfs_runtime().await;

    // Build a version 1.0.0 of some package.
    build_package!(
        tmpdir,
        "dependency.spk.yaml",
        br#"
pkg: dependency/1.0.0

build:
  script:
    - "true"
"#
    );

    // Build a package that depends on "dependency".
    let package_filename = build_package!(
        tmpdir,
        "package.spk.yaml",
        br#"
pkg: package/1.0.0

build:
  options:
    - pkg: dependency
  script:
    - "true"
"#,
    );

    // Now build a newer version of the dependency.
    build_package!(
        tmpdir,
        "dependency.spk.yaml",
        br#"
pkg: dependency/1.0.1

build:
  script:
    - "true"
"#,
    );

    // And build the other package again.
    build_package!(tmpdir, package_filename);

    // The second time building "package" we expect it to build something with
    // the _same_ build digest (e.g., the change in version of one of its
    // dependencies shouldn't affect the build digest). Verify this by checking
    // that there is still only one build of this package.

    let ident = version_ident!("package/1.0.0");

    let non_src_builds = rt
        .tmprepo
        .list_package_builds(&ident)
        .await
        .unwrap()
        .into_iter()
        .filter(|b| !b.is_source());

    assert_eq!(non_src_builds.count(), 1, "Expected one build");
}

#[rstest]
#[case::cli("cli")]
#[case::checks("checks")]
#[tokio::test]
async fn test_build_with_circular_dependency(
    tmpdir: tempfile::TempDir,
    #[case] solver_to_run: &str,
) {
    // The system should not allow a package to be built that has a circular
    // dependency.
    let _rt = spfs_runtime().await;

    // Start out with a package with no dependencies.
    let (_, r) = try_build_package!(
        tmpdir,
        "one.spk.yaml",
        br#"
pkg: one/1.0.0

build:
  script:
    - "true"
"#
        "--solver-to-run",
        solver_to_run
    );

    r.expect("Expected initial build of one to succeed");

    // Build a package that depends on "one".
    let (_, r) = try_build_package!(
        tmpdir,
        "two.spk.yaml",
        br#"
pkg: two/1.0.0

build:
  options:
    - pkg: one
  script:
    - "true"

install:
  requirements:
    - pkg: one
      fromBuildEnv: true
"#,
        "--solver-to-run",
        solver_to_run
    );

    r.expect("Expected build of two to succeed");

    // Now build a newer version of "one" that depends on "two".
    let (_, r) = try_build_package!(
        tmpdir,
        "one.spk.yaml",
        br#"
pkg: one/1.0.0

build:
  options:
    - pkg: two
  script:
    - "true"

install:
  requirements:
    - pkg: two
      fromBuildEnv: true
"#,
        "--solver-to-run",
        solver_to_run
    );

    r.expect_err("Expected build to fail");
}

#[rstest]
#[case::cli("cli")]
#[case::checks("checks")]
#[tokio::test]
async fn test_build_with_circular_dependency_allow_with_flag(
    tmpdir: tempfile::TempDir,
    #[case] solver_to_run: &str,
) {
    // The system should not allow a package to be built that has a circular
    // dependency.
    let _rt = spfs_runtime().await;

    // Start out with a package with no dependencies.
    let (_, r) = try_build_package!(
        tmpdir,
        "one.spk.yaml",
        br#"
pkg: one/1.0.0

build:
  script:
    - "true"
"#
        "--solver-to-run",
        solver_to_run
    );

    r.expect("Expected initial build of one to succeed");

    // Build a package that depends on "one".
    let (_, r) = try_build_package!(
        tmpdir,
        "two.spk.yaml",
        br#"
pkg: two/1.0.0

build:
  options:
    - pkg: one
  script:
    - "true"

install:
  requirements:
    - pkg: one
      fromBuildEnv: true
"#,
        "--solver-to-run",
        solver_to_run
    );

    r.expect("Expected build of two to succeed");

    // Now build a newer version of "one" that depends on "two".
    let (_, r) = try_build_package!(
        tmpdir,
        "one.spk.yaml",
        br#"
pkg: one/1.0.0

build:
  options:
    - pkg: two
  script:
    - "true"

install:
  requirements:
    - pkg: two
      fromBuildEnv: true
"#,
        "--solver-to-run",
        solver_to_run,
        "--allow-circular-dependencies"
    );

    r.expect("Expected build of one to succeed");
}
