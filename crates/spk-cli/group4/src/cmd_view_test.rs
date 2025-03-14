// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::fs::File;
use std::io::Write;

use clap::Parser;
use rstest::{fixture, rstest};
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
