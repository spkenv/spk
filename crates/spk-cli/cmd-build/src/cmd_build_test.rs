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

    let filename = tmpdir.path().join("three-variants.spk.yaml");
    {
        let mut file = File::create(&filename).unwrap();
        file.write_all(
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
        )
        .unwrap();
    }

    let filename_str = filename.as_os_str().to_str().unwrap();

    let mut opt = Opt::try_parse_from([
        "build",
        // Don't exec a new process to move into a new runtime, this confuses
        // coverage testing.
        "--no-runtime",
        "--disable-repo=origin",
        filename_str,
    ])
    .unwrap();
    opt.build.run().await.unwrap();

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
    {
        let filename = tmpdir.path().join("old.spk.yaml");
        {
            let mut file = File::create(&filename).unwrap();
            file.write_all(
                br#"
pkg: dependency/1.0.0

build:
  script:
    - "true"
"#,
            )
            .unwrap();
        }

        let filename_str = filename.as_os_str().to_str().unwrap();

        let mut opt = Opt::try_parse_from([
            "build",
            // Don't exec a new process to move into a new runtime, this confuses
            // coverage testing.
            "--no-runtime",
            "--disable-repo=origin",
            filename_str,
        ])
        .unwrap();
        opt.build.run().await.unwrap();
    }

    // Build a package that depends on "dependency".
    let package_filename = {
        let filename = tmpdir.path().join("package.spk.yaml");
        {
            let mut file = File::create(&filename).unwrap();
            file.write_all(
                br#"
pkg: package/1.0.0

build:
  options:
    - pkg: dependency
  script:
    - "true"
"#,
            )
            .unwrap();
        }

        let filename_str = filename.as_os_str().to_str().unwrap();

        let mut opt = Opt::try_parse_from([
            "build",
            // Don't exec a new process to move into a new runtime, this confuses
            // coverage testing.
            "--no-runtime",
            "--disable-repo=origin",
            filename_str,
        ])
        .unwrap();
        opt.build.run().await.unwrap();

        filename
    };

    // Now build a newer version of the dependency.
    {
        let filename = tmpdir.path().join("old.spk.yaml");
        {
            let mut file = File::create(&filename).unwrap();
            file.write_all(
                br#"
pkg: dependency/1.0.1

build:
  script:
    - "true"
"#,
            )
            .unwrap();
        }

        let filename_str = filename.as_os_str().to_str().unwrap();

        let mut opt = Opt::try_parse_from([
            "build",
            // Don't exec a new process to move into a new runtime, this confuses
            // coverage testing.
            "--no-runtime",
            "--disable-repo=origin",
            filename_str,
        ])
        .unwrap();
        opt.build.run().await.unwrap();
    }

    // And build the other package again.
    {
        let filename_str = package_filename.as_os_str().to_str().unwrap();

        let mut opt = Opt::try_parse_from([
            "build",
            // Don't exec a new process to move into a new runtime, this confuses
            // coverage testing.
            "--no-runtime",
            "--disable-repo=origin",
            filename_str,
        ])
        .unwrap();
        opt.build.run().await.unwrap();
    }

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
