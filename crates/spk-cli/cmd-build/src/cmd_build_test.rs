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
use spk_schema::ident_component::Component;
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

    // Force middle to pick up exactly 1.0.0 so for the multiple builds below
    // it doesn't pick up an already-built 1.0.1 of circ and the contents of
    // version.txt will still be "1.0.0" during the build of circ.
    build_package!(
        tmpdir,
        "middle.spk.yaml",
        br#"
pkg: middle/1.0.0

build:
  options:
    - pkg: circ/=1.0.0
  script:
    - "true"

install:
  requirements:
    - pkg: circ/=1.0.0
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

#[rstest]
#[tokio::test]
async fn test_package_with_circular_dep_can_build_major_version_change(tmpdir: tempfile::TempDir) {
    // A package that depends on itself should be able to build a new major
    // version of itself, as in something not compatible with the version
    // being brought in via the circular dependency.
    let _rt = spfs_runtime().await;

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

    // Attempt to build a 2.0.0 version of circ, which shouldn't prevent
    // middle from being able to resolve the 1.0.0 version of circ.
    build_package!(
        tmpdir,
        "circ.spk.yaml",
        br#"
pkg: circ/2.0.0

build:
  options:
    - pkg: middle
  script:
    - echo "2.0.0" > $PREFIX/version.txt
"#,
    );
}

#[rstest]
#[tokio::test]
async fn test_package_with_circular_dep_collects_all_files(tmpdir: tempfile::TempDir) {
    // Building a new version of a package that depends on itself should
    // produce a package containing all the expected files, even if the new
    // build creates files with the same content as the previous build.
    let rt = spfs_runtime().await;

    build_package!(
        tmpdir,
        "circ.spk.yaml",
        br#"
pkg: circ/1.0.0

build:
  script:
    - echo "1.0.0" > $PREFIX/version.txt
    - echo "hello world" > $PREFIX/hello.txt
    - echo "unchanged" > $PREFIX/unchanged.txt
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

    // This build overwrites a file from the previous build, but it has the same
    // contents. It should still be detected as a file that needs to be part of
    // the newly make package.
    build_package!(
        tmpdir,
        "circ.spk.yaml",
        br#"
pkg: circ/2.0.0

build:
  options:
    - pkg: middle
  script:
    - echo "2.0.0" > $PREFIX/version.txt
    - echo "hello world" > $PREFIX/hello.txt
"#,
    );

    let build = rt
        .tmprepo
        .list_package_builds(&version_ident!("circ/2.0.0"))
        .await
        .unwrap()
        .into_iter()
        .find(|b| !b.is_source())
        .unwrap();

    let digest = *rt
        .tmprepo
        .read_components(&build)
        .await
        .unwrap()
        .get(&Component::Run)
        .unwrap();

    let spk_storage::RepositoryHandle::SPFS(repo) = &*rt.tmprepo else {
        panic!("Expected SPFS repo");
    };

    let layer = repo.read_layer(digest).await.unwrap();

    let manifest = repo
        .read_manifest(layer.manifest)
        .await
        .unwrap()
        .to_tracking_manifest();

    let entry = manifest.get_path("hello.txt");
    assert!(
        entry.is_some(),
        "should capture file created in build but unmodified from previous build"
    );
    let entry = manifest.get_path("unchanged.txt");
    assert!(
        entry.is_none(),
        "should not capture file from old build that was not modified in new build"
    );
}
