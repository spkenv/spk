// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use clap::Parser;
use spfs::RemoteAddress;
use spfs::config::Remote;
use spk_schema::foundation::ident_component::Component;
use spk_schema::{recipe, spec};
use spk_storage::fixtures::*;

use super::{Output, Run, Stats};
use crate::cmd_stats::{ALL_PACKAGES_WAIT_MESSAGE, ONE_PACKAGE_WAIT_MESSAGE};

#[derive(Default)]
struct OutputToVec {
    vec: Vec<String>,
    warnings: Vec<String>,
}

impl Output for OutputToVec {
    fn println(&mut self, line: String) {
        self.vec.push(line);
    }

    fn warn(&mut self, line: String) {
        self.warnings.push(line);
    }
}

#[derive(Parser)]
struct Opt {
    #[clap(flatten)]
    stats: Stats<OutputToVec>,
}

#[tokio::test]
async fn test_stats_on_empty_repo() {
    let mut rt = spfs_runtime().await;
    let remote_repo = spfsrepo().await;

    rt.add_remote_repo(
        "origin",
        Remote::Address(RemoteAddress {
            address: remote_repo.address().clone(),
        }),
    )
    .unwrap();

    let mut opt = Opt::try_parse_from(["stats", "--show-top", "15"]).unwrap();
    let result = opt.stats.run().await.unwrap();

    assert_eq!(result, 0);
    assert_ne!(opt.stats.output.vec.len(), 0);
    assert!(
        opt.stats
            .output
            .vec
            .contains(&ALL_PACKAGES_WAIT_MESSAGE.to_string())
    );
}

#[tokio::test]
async fn test_stats() {
    let mut rt = spfs_runtime().await;
    let remote_repo = spfsrepo().await;

    // Set up a repo with one package.
    rt.add_remote_repo(
        "origin",
        Remote::Address(RemoteAddress {
            address: remote_repo.address().clone(),
        }),
    )
    .unwrap();

    let recipe = recipe!({"pkg": "my-pkg/1.0.0"});
    remote_repo.publish_recipe(&recipe).await.unwrap();
    let spec = spec!({"pkg": "my-pkg/1.0.0/BGSHW3CN"});
    remote_repo
        .publish_package(
            &spec,
            &vec![(Component::Run, empty_layer_digest())]
                .into_iter()
                .collect(),
        )
        .await
        .unwrap();

    let mut opt = Opt::try_parse_from(["stats"]).unwrap();
    let result = opt.stats.run().await.unwrap();

    assert_eq!(result, 0);
    assert_ne!(opt.stats.output.vec.len(), 0);

    assert!(
        opt.stats
            .output
            .vec
            .contains(&ALL_PACKAGES_WAIT_MESSAGE.to_string())
    );
}

#[tokio::test]
async fn test_stats_single_package() {
    let mut rt = spfs_runtime().await;
    let remote_repo = spfsrepo().await;

    // Set up a repo with one package.
    rt.add_remote_repo(
        "origin",
        Remote::Address(RemoteAddress {
            address: remote_repo.address().clone(),
        }),
    )
    .unwrap();

    let recipe = recipe!({"pkg": "my-pkg/1.0.0"});
    remote_repo.publish_recipe(&recipe).await.unwrap();
    let spec = spec!({"pkg": "my-pkg/1.0.0/BGSHW3CN"});
    remote_repo
        .publish_package(
            &spec,
            &vec![(Component::Run, empty_layer_digest())]
                .into_iter()
                .collect(),
        )
        .await
        .unwrap();

    let mut opt = Opt::try_parse_from(["stats", "my-pkg"]).unwrap();
    let result = opt.stats.run().await.unwrap();

    assert_eq!(result, 0);
    assert_ne!(opt.stats.output.vec.len(), 0);

    assert!(
        opt.stats
            .output
            .vec
            .contains(&ONE_PACKAGE_WAIT_MESSAGE.to_string())
    );
}

#[tokio::test]
async fn test_stats_single_package_with_deprecated() {
    let mut rt = spfs_runtime().await;
    let remote_repo = spfsrepo().await;

    // Set up a repo with one package.
    rt.add_remote_repo(
        "origin",
        Remote::Address(RemoteAddress {
            address: remote_repo.address().clone(),
        }),
    )
    .unwrap();

    let recipe = recipe!({"pkg": "my-pkg/1.0.0"});
    remote_repo.publish_recipe(&recipe).await.unwrap();
    let spec = spec!({"pkg": "my-pkg/1.0.0/BGSHW3CN",
                      "deprecated": true});
    remote_repo
        .publish_package(
            &spec,
            &vec![(Component::Run, empty_layer_digest())]
                .into_iter()
                .collect(),
        )
        .await
        .unwrap();
    let spec2 = spec!({"pkg": "my-pkg/1.1.0/BGSHW3CN"});
    remote_repo
        .publish_package(
            &spec2,
            &vec![(Component::Run, empty_layer_digest())]
                .into_iter()
                .collect(),
        )
        .await
        .unwrap();

    let mut opt = Opt::try_parse_from(["stats", "my-pkg", "--deprecated"]).unwrap();
    let result = opt.stats.run().await.unwrap();

    assert_eq!(result, 0);
    assert_ne!(opt.stats.output.vec.len(), 0);

    assert!(
        opt.stats
            .output
            .vec
            .contains(&ONE_PACKAGE_WAIT_MESSAGE.to_string())
    );
}
