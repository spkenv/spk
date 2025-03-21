// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::fs::File;
use std::io::Write;

use clap::Parser;
use rstest::{fixture, rstest};
use spfs::runtime::makedirs_with_perms;
use spk_cli_common::Run;

use super::View;

#[derive(Parser)]
struct Opt {
    #[clap(flatten)]
    view: View,
}

#[fixture]
pub fn tmpdir() -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix("spk-test-")
        .tempdir()
        .expect("create a temp directory for test files")
}

pub fn init_logging() {
    let sub = tracing_subscriber::FmtSubscriber::builder()
        .with_env_filter("spk_workspace=trace,debug")
        .without_time()
        .with_test_writer()
        .finish();
    let _ = tracing::subscriber::set_global_default(sub);
}

// The use of the --workspace flag in these tests is meant to simulate the
// default behavior without the flag, which defaults to using ".", but these
// tests want to avoid changing the current working directory during test
// execution, so the tests don't need to be serialized or have to worry about
// changing the current working directory back to the original value.

#[rstest]
#[tokio::test]
async fn view_on_filename_and_default_workspace(tmpdir: tempfile::TempDir) {
    init_logging();

    let full_name = tmpdir.path().join("package.spk.yaml");

    let mut file = File::create(&full_name).unwrap();
    file.write_all(
        r#"
pkg: test/1.0.0
api: v0/package
"#
        .as_bytes(),
    )
    .unwrap();

    let mut opt = Opt::try_parse_from([
        "view",
        "-vvv",
        "--workspace",
        &tmpdir.path().to_string_lossy(),
        // A straight `spk info package.spk.yaml` fails with "Failed to parse
        // request" and/or "yaml was expected to contain a list of requests"
        // but using the `--variants` flag still does something expected.
        "--variants",
        &full_name.to_string_lossy(),
    ])
    .unwrap();
    opt.view.run().await.unwrap();
}

#[rstest]
#[tokio::test]
async fn view_on_filename_and_default_workspace_and_garbage_file(tmpdir: tempfile::TempDir) {
    init_logging();

    let full_name = tmpdir.path().join("package.spk.yaml");

    let mut file = File::create(&full_name).unwrap();
    file.write_all(
        r#"
pkg: test/1.0.0
api: v0/package
"#
        .as_bytes(),
    )
    .unwrap();

    let mut file = File::create(tmpdir.path().join("garbage.spk.yaml")).unwrap();
    file.write_all(
        r#"
this isn't a recipe file
"#
        .as_bytes(),
    )
    .unwrap();

    let mut opt = Opt::try_parse_from([
        "view",
        "-vvv",
        "--workspace",
        &tmpdir.path().to_string_lossy(),
        // A straight `spk info package.spk.yaml` fails with "Failed to parse
        // request" and/or "yaml was expected to contain a list of requests"
        // but using the `--variants` flag still does something expected.
        "--variants",
        &full_name.to_string_lossy(),
    ])
    .unwrap();
    opt.view.run().await.unwrap();
}

#[rstest]
#[tokio::test]
async fn view_on_filename_in_sibling_dir_and_default_workspace(
    #[from(tmpdir)] tmpdir1: tempfile::TempDir,
    #[from(tmpdir)] tmpdir2: tempfile::TempDir,
) {
    init_logging();

    let full_name = tmpdir1.path().join("package.spk.yaml");

    let mut file = File::create(&full_name).unwrap();
    file.write_all(
        r#"
pkg: test/1.0.0
api: v0/package
"#
        .as_bytes(),
    )
    .unwrap();

    let mut opt = Opt::try_parse_from([
        "view",
        "-vvv",
        "--workspace",
        &tmpdir2.path().to_string_lossy(),
        // A straight `spk info package.spk.yaml` fails with "Failed to parse
        // request" and/or "yaml was expected to contain a list of requests"
        // but using the `--variants` flag still does something expected.
        "--variants",
        &full_name.to_string_lossy(),
    ])
    .unwrap();
    opt.view.run().await.unwrap();
}

#[rstest]
#[tokio::test]
async fn view_on_filename_in_subdir_and_default_workspace(tmpdir: tempfile::TempDir) {
    init_logging();

    makedirs_with_perms(tmpdir.path().join("subdir"), 0o777).unwrap();

    let full_name = tmpdir.path().join("subdir").join("package.spk.yaml");

    let mut file = File::create(&full_name).unwrap();
    file.write_all(
        r#"
pkg: test/1.0.0
api: v0/package
"#
        .as_bytes(),
    )
    .unwrap();

    let mut opt = Opt::try_parse_from([
        "view",
        "-vvv",
        "--workspace",
        &tmpdir.path().to_string_lossy(),
        // A straight `spk info package.spk.yaml` fails with "Failed to parse
        // request" and/or "yaml was expected to contain a list of requests"
        // but using the `--variants` flag still does something expected.
        "--variants",
        &full_name.to_string_lossy(),
    ])
    .unwrap();
    opt.view.run().await.unwrap();
}

#[rstest]
#[tokio::test]
async fn view_on_filename_and_unrelated_workspace(tmpdir: tempfile::TempDir) {
    init_logging();

    let full_name = tmpdir.path().join("package.spk.yaml");

    let mut file = File::create(&full_name).unwrap();
    file.write_all(
        r#"
pkg: test/1.0.0
api: v0/package
"#
        .as_bytes(),
    )
    .unwrap();

    // This workspace intentionally does not reference package.spk.yaml.
    let mut file = File::create(tmpdir.path().join("workspace.spk.yaml")).unwrap();
    file.write_all(
        r#"
api: v0/workspace
recipes: []
"#
        .as_bytes(),
    )
    .unwrap();

    let mut opt = Opt::try_parse_from([
        "view",
        "-vvv",
        "--workspace",
        &tmpdir.path().to_string_lossy(),
        // A straight `spk info package.spk.yaml` fails with "Failed to parse
        // request" and/or "yaml was expected to contain a list of requests"
        // but using the `--variants` flag still does something expected.
        "--variants",
        &full_name.to_string_lossy(),
    ])
    .unwrap();
    opt.view.run().await.unwrap();
}

#[rstest]
#[tokio::test]
async fn view_on_workspace_filename_and_existing_workspace(tmpdir: tempfile::TempDir) {
    init_logging();

    let full_name = tmpdir.path().join("workspace.spk.yaml");

    let mut file = File::create(&full_name).unwrap();
    file.write_all(
        r#"
api: v0/workspace
recipes: []
"#
        .as_bytes(),
    )
    .unwrap();

    let mut opt = Opt::try_parse_from([
        "view",
        "-vvv",
        "--workspace",
        &tmpdir.path().to_string_lossy(),
        // A straight `spk info package.spk.yaml` fails with "Failed to parse
        // request" and/or "yaml was expected to contain a list of requests"
        // but using the `--variants` flag still does something expected.
        //
        // Doing `--variants` on a workspace file is meaningless but this test
        // demonstrates how this fails with a "file does not exist" error when
        // the file does exist.
        "--variants",
        &full_name.to_string_lossy(),
    ])
    .unwrap();
    let err = opt
        .view
        .run()
        .await
        .expect_err("--variants should fail on a workspace file");

    assert!(
        !err.to_string().contains("did not find package template"),
        "Expected error to not contain 'did not find package template', got: {}",
        err,
    );
}
