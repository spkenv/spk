// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use clap::Parser;
use rstest::rstest;
use spfs::RemoteAddress;
use spfs::config::Remote;
use spk_cli_common::Run;
use spk_solve::{Component, recipe, spec};
use spk_storage::fixtures::{empty_layer_digest, spfs_runtime, spfsrepo};

use super::Bake;

#[derive(Parser)]
struct Opt {
    #[clap(flatten)]
    bake: Bake,
}

#[rstest]
#[tokio::test]
async fn test_bake() {
    // Test the bake command runs
    let mut rt = spfs_runtime().await;
    let remote_repo = spfsrepo().await;

    // Populate the "origin" repo with one package.
    // The "local" repo is empty.
    rt.add_remote_repo(
        "origin",
        Remote::Address(RemoteAddress {
            address: remote_repo.address().clone(),
        }),
    )
    .unwrap();

    let recipe = recipe!({"pkg": "my-pkg/1.0.1"});
    remote_repo.publish_recipe(&recipe).await.unwrap();
    let spec = spec!({"pkg": "my-pkg/1.0.1/ZPGKGOTY"});
    remote_repo
        .publish_package(
            &spec,
            &vec![(Component::Run, empty_layer_digest())]
                .into_iter()
                .collect(),
        )
        .await
        .unwrap();

    // Test a basic bake
    let mut opt = Opt::try_parse_from(["bake", "--no-runtime", "my-pkg:run"]).unwrap();
    let result = opt.bake.run().await.unwrap();
    assert_eq!(result, 0);
}

#[rstest]
#[tokio::test]
async fn test_bake_incompatible_merged_request() {
    // Test bake with an incompatible set of requests
    let mut rt = spfs_runtime().await;
    let remote_repo = spfsrepo().await;

    // Populate the "origin" repo with one package.
    // The "local" repo is empty.
    rt.add_remote_repo(
        "origin",
        Remote::Address(RemoteAddress {
            address: remote_repo.address().clone(),
        }),
    )
    .unwrap();

    let recipe = recipe!({"pkg": "my-pkg/1.0.33+r.1"});
    remote_repo.publish_recipe(&recipe).await.unwrap();
    let spec = spec!({"pkg": "my-pkg/1.0.33+r.1/ZPGKGOTY"});
    remote_repo
        .publish_package(
            &spec,
            &vec![(Component::Run, empty_layer_digest())]
                .into_iter()
                .collect(),
        )
        .await
        .unwrap();

    // Test bake command with 2 incompatible requests. This should
    // not panic, it should error out
    let mut opt = Opt::try_parse_from([
        "bake",
        "--no-runtime",
        "my-pkg:run/==1.0.33+r.1/ZPGKGOTY",
        "my-pkg:run/=1.0.99",
    ])
    .unwrap();
    let result = opt.bake.run().await;
    println!("bake run result: {result:?}");

    match result {
        Err(err) => {
            println!("Bake errored with: {err}");
        }
        Ok(_value) => {
            panic!("Incompatible requests for same package should cause bake to error");
        }
    }
}
