// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use clap::Parser;
use rstest::rstest;
use spfs::Config;
use spfs::fixtures::*;
use spfs::prelude::*;

use super::CmdInfo;

#[derive(Parser)]
struct Opt {
    #[command(flatten)]
    info: CmdInfo,
}

#[rstest]
#[case::fs(tmprepo("fs"))]
#[tokio::test]
async fn info_on_payload(
    #[case]
    #[future]
    repo: TempRepo,
) {
    let repo = repo.await;

    let manifest = generate_tree(&repo).await.to_graph_manifest();
    let file = manifest
        .iter_entries()
        .find(|entry| entry.is_regular_file())
        .expect("at least one regular file");

    let mut opt = Opt::try_parse_from([
        "info",
        "-r",
        &repo.address().to_string(),
        &file.object().to_string(),
    ])
    .unwrap();
    let config = Config::default();
    let code = opt
        .info
        .run(&config)
        .await
        .expect("`spfs info` on a file digest is successful");
    assert_eq!(code, 0, "`spfs info` on a file digest returns exit code 0");
}
