// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::fs::File;
use std::io::Write;

use clap::Parser;
use rstest::rstest;
use spk_cli_common::Run;
use spk_schema::foundation::fixtures::*;
use spk_storage::fixtures::*;

use super::MakeBinary;

#[derive(Parser)]
struct Opt {
    #[clap(flatten)]
    mkb: MakeBinary,
}

#[rstest]
#[tokio::test]
async fn test_build_options_are_respected(tmpdir: tempfile::TempDir) {
    let _rt = spfs_runtime().await;

    let filename = tmpdir.path().join("simple.spk.yaml");
    {
        let mut file = File::create(&filename).unwrap();
        file.write_all(
            br#"
pkg: simple/1.0.0

build:
  options:
    - var: variable/default
  script:
    - if [ "$SPK_OPT_variable" = "override" ]; then exit 0; fi
    - echo 'Expected $SPK_OPT_variable value to be overridden!'
    - exit 1
"#,
        )
        .unwrap();
    }

    let filename_str = filename.as_os_str().to_str().unwrap();

    // First, don't override the variable on the command line (test the test).
    let mut opt = Opt::try_parse_from([
        "make-binary",
        // Don't exec a new process to move into a new runtime, this confuses
        // coverage testing.
        "--no-runtime",
        "--disable-repo=origin",
        "--here",
        filename_str,
    ])
    .unwrap();
    assert!(
        opt.mkb.run().await.is_err(),
        "Without override, build script should fail."
    );

    let mut opt = Opt::try_parse_from([
        "make-binary",
        // Don't exec a new process to move into a new runtime, this confuses
        // coverage testing.
        "--no-runtime",
        "--disable-repo=origin",
        "--here",
        // Override the value of var "variable" on the command line.
        "--opt",
        "variable=override",
        filename_str,
    ])
    .unwrap();
    opt.mkb
        .run()
        .await
        .expect("With override, build script should succeed.");
}
