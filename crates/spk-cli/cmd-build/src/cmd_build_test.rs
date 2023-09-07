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
#[tokio::test]
async fn test_package_with_circular_dep_can_modify_files(tmpdir: tempfile::TempDir) {
    // A package that depends on itself should be able to modify files
    // belonging to itself.
    let _rt = spfs_runtime().await;

    build_package!(
        tmpdir,
        "other.spk.yaml",
        br#"
pkg: other/1.0.0

build:
  script:
    - echo "1.0.0" > $PREFIX/a.txt
    - echo "1.0.0" > $PREFIX/z.txt
"#
    );

    build_package!(
        tmpdir,
        "circ.spk.yaml",
        br#"
pkg: circ/1.0.0

build:
  script:
    - echo "1.0.0" > $PREFIX/version.txt
"#
    );

    build_package!(
        tmpdir,
        "middle.spk.yaml",
        br#"
pkg: middle/1.0.0

build:
  options:
    - pkg: circ
  script:
    - "true"

install:
  requirements:
    - pkg: circ
      fromBuildEnv: true
"#,
    );

    // Attempt to build a newer version of circ, but now it depends on `middle`
    // creating a circular dependency. This build should succeed even though it
    // modifies a file belonging to "existing files" because the file it
    // modifies belongs to [a different version of] the same package as is
    // being built.
    build_package!(
        tmpdir,
        "circ.spk.yaml",
        br#"
pkg: circ/1.0.1

build:
  options:
    - pkg: middle
  script:
    # this test is only valid if $PREFIX/version.txt exists already
    - test -f $PREFIX/version.txt
    - echo "1.0.1" > $PREFIX/version.txt
"#,
    );

    for other_file in ["a", "z"] {
        // Attempt to build a new version of circ but also modify a file belonging
        // to some other package. This should still be caught as an illegal
        // operation.
        //
        // We attempt this twice with two different filenames, one that sorts
        // before "version.txt" and one that sorts after, to exercise the case
        // where modifying the file from our own package is encountered first,
        // to prove that even though it allows the first modification, it still
        // checks for more.
        try_build_package!(
            tmpdir,
            "circ.spk.yaml",
            format!(
                r#"
pkg: circ/1.0.1

build:
  options:
    - pkg: middle
    - pkg: other
  script:
    # this test is only valid if $PREFIX/version.txt exists already
    - test -f $PREFIX/version.txt
    - echo "1.0.1" > $PREFIX/version.txt
    # try to modify a file belonging to 'other' too
    - echo "1.0.1" > $PREFIX/{other_file}.txt
"#
            )
            .as_bytes(),
        )
        .expect_err("Expected build to fail");
    }
}
