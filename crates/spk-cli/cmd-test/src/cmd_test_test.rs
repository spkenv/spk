// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::fs::File;
use std::io::Write;

use clap::Parser;
use rstest::rstest;
use spk_cli_common::Run;
use spk_cmd_build::cmd_build::Build;
use spk_schema::foundation::fixtures::*;
use spk_storage::fixtures::*;

use super::Test;

#[derive(Parser)]
struct BuildOpt {
    #[clap(flatten)]
    build: Build,
}

#[derive(Parser)]
struct TestOpt {
    #[clap(flatten)]
    test: Test,
}

#[rstest]
#[tokio::test]
async fn test_all_test_stages_succeed(tmpdir: tempfile::TempDir) {
    // A var that appears in the variant list and doesn't appear in the
    // build.options list should still affect the build hash / produce a
    // unique build.
    let _rt = spfs_runtime().await;

    let filename = tmpdir.path().join("simple.spk.yaml");
    {
        let mut file = File::create(&filename).unwrap();
        file.write_all(
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
        )
        .unwrap();
    }

    let filename_str = filename.as_os_str().to_str().unwrap();

    // Build the package so it can be tested.
    let mut opt = BuildOpt::try_parse_from([
        "build",
        // Don't exec a new process to move into a new runtime, this confuses
        // coverage testing.
        "--no-runtime",
        "--disable-repo=origin",
        filename_str,
    ])
    .unwrap();
    opt.build.run().await.unwrap();

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
