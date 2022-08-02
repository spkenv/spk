// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use clap::Parser;

use spfs::{config::Remote, RemoteAddress};
use spk::{api, fixtures::*, spec};

use super::{Ls, Output, Run};

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
    ls: Ls<OutputToVec>,
}

#[tokio::test]
async fn test_ls_trivially_works() {
    let mut rt = spfs_runtime().await;
    let remote_repo = spfsrepo().await;

    rt.add_remote_repo(
        "origin",
        Remote::Address(RemoteAddress {
            address: remote_repo.address().clone(),
        }),
    )
    .unwrap();

    let mut opt = Opt::try_parse_from([] as [&str; 0]).unwrap();
    opt.ls.run().await.unwrap();
    assert_eq!(opt.ls.output.vec.len(), 0);
}

/// `spk ls` is expected to list packages in the configured remote
/// repositories.
#[tokio::test]
async fn test_ls_shows_remote_packages() {
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

    let spec = spec!({"pkg": "my-pkg/1.0.0"});
    remote_repo.publish_spec(&spec).await.unwrap();
    let spec = spec!({"pkg": "my-pkg/1.0.0/BGSHW3CN"});
    remote_repo
        .publish_package(
            &spec,
            vec![(api::Component::Run, empty_layer_digest())]
                .into_iter()
                .collect(),
        )
        .await
        .unwrap();

    let mut opt = Opt::try_parse_from([] as [&str; 0]).unwrap();
    opt.ls.run().await.unwrap();
    assert_ne!(opt.ls.output.vec.len(), 0);
}

/// `spk ls` is expected to list packages in both the local and the configured
/// remote repositories.
#[tokio::test]
async fn test_ls_shows_local_and_remote_packages() {
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

    let spec = spec!({"pkg": "my-remote-pkg/1.0.0"});
    remote_repo.publish_spec(&spec).await.unwrap();
    let spec = spec!({"pkg": "my-remote-pkg/1.0.0/BGSHW3CN"});
    remote_repo
        .publish_package(
            &spec,
            vec![(api::Component::Run, empty_layer_digest())]
                .into_iter()
                .collect(),
        )
        .await
        .unwrap();

    let spec = spec!({"pkg": "my-local-pkg/1.0.0"});
    rt.tmprepo.publish_spec(&spec).await.unwrap();
    let spec = spec!({"pkg": "my-local-pkg/1.0.0/BGSHW3CN"});
    rt.tmprepo
        .publish_package(
            &spec,
            vec![(api::Component::Run, empty_layer_digest())]
                .into_iter()
                .collect(),
        )
        .await
        .unwrap();

    let mut opt = Opt::try_parse_from([] as [&str; 0]).unwrap();
    opt.ls.run().await.unwrap();
    assert_eq!(opt.ls.output.vec.len(), 2);
}

/// `spk ls -l` is expected to list packages in only the local repository.
#[tokio::test]
async fn test_ls_dash_l_shows_local_packages_only() {
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

    let spec = spec!({"pkg": "my-remote-pkg/1.0.0"});
    remote_repo.publish_spec(&spec).await.unwrap();
    let spec = spec!({"pkg": "my-remote-pkg/1.0.0/BGSHW3CN"});
    remote_repo
        .publish_package(
            &spec,
            vec![(api::Component::Run, empty_layer_digest())]
                .into_iter()
                .collect(),
        )
        .await
        .unwrap();

    let spec = spec!({"pkg": "my-local-pkg/1.0.0"});
    rt.tmprepo.publish_spec(&spec).await.unwrap();
    let spec = spec!({"pkg": "my-local-pkg/1.0.0/BGSHW3CN"});
    rt.tmprepo
        .publish_package(
            &spec,
            vec![(api::Component::Run, empty_layer_digest())]
                .into_iter()
                .collect(),
        )
        .await
        .unwrap();

    let mut opt = Opt::try_parse_from(["ls", "-l"]).unwrap();
    opt.ls.run().await.unwrap();
    assert_eq!(opt.ls.output.vec.len(), 1);
    assert_eq!(opt.ls.output.vec.get(0).unwrap(), "my-local-pkg");
}

/// `spk ls -r origin` is expected to list packages in both the origin
/// and local repositories.
#[tokio::test]
async fn test_ls_dash_r_shows_local_and_remote_packages() {
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

    let spec = spec!({"pkg": "my-remote-pkg/1.0.0"});
    remote_repo.publish_spec(&spec).await.unwrap();
    let spec = spec!({"pkg": "my-remote-pkg/1.0.0/BGSHW3CN"});
    remote_repo
        .publish_package(
            &spec,
            vec![(api::Component::Run, empty_layer_digest())]
                .into_iter()
                .collect(),
        )
        .await
        .unwrap();

    let spec = spec!({"pkg": "my-local-pkg/1.0.0"});
    rt.tmprepo.publish_spec(&spec).await.unwrap();
    let spec = spec!({"pkg": "my-local-pkg/1.0.0/BGSHW3CN"});
    rt.tmprepo
        .publish_package(
            &spec,
            vec![(api::Component::Run, empty_layer_digest())]
                .into_iter()
                .collect(),
        )
        .await
        .unwrap();

    let mut opt = Opt::try_parse_from(["ls", "-r", "origin"]).unwrap();
    opt.ls.run().await.unwrap();
    assert_eq!(opt.ls.output.vec.len(), 2);
}

/// `spk ls --no-local-repo` is expected to list packages in only the remote
/// repositories.
#[tokio::test]
async fn test_ls_dash_dash_no_local_repo_shows_remote_packages_only() {
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

    let spec = spec!({"pkg": "my-remote-pkg/1.0.0"});
    remote_repo.publish_spec(&spec).await.unwrap();
    let spec = spec!({"pkg": "my-remote-pkg/1.0.0/BGSHW3CN"});
    remote_repo
        .publish_package(
            &spec,
            vec![(api::Component::Run, empty_layer_digest())]
                .into_iter()
                .collect(),
        )
        .await
        .unwrap();

    let spec = spec!({"pkg": "my-local-pkg/1.0.0"});
    rt.tmprepo.publish_spec(&spec).await.unwrap();
    let spec = spec!({"pkg": "my-local-pkg/1.0.0/BGSHW3CN"});
    rt.tmprepo
        .publish_package(
            &spec,
            vec![(api::Component::Run, empty_layer_digest())]
                .into_iter()
                .collect(),
        )
        .await
        .unwrap();

    let mut opt = Opt::try_parse_from(["ls", "--no-local-repo"]).unwrap();
    opt.ls.run().await.unwrap();
    assert_eq!(opt.ls.output.vec.len(), 1);
    assert_eq!(opt.ls.output.vec.get(0).unwrap(), "my-remote-pkg");
}

/// `spk ls --disable-repo origin` is expected to list packages in only the
/// local repository.
#[tokio::test]
async fn test_ls_dash_dash_disable_repo_shows_local_packages_only() {
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

    let spec = spec!({"pkg": "my-remote-pkg/1.0.0"});
    remote_repo.publish_spec(&spec).await.unwrap();
    let spec = spec!({"pkg": "my-remote-pkg/1.0.0/BGSHW3CN"});
    remote_repo
        .publish_package(
            &spec,
            vec![(api::Component::Run, empty_layer_digest())]
                .into_iter()
                .collect(),
        )
        .await
        .unwrap();

    let spec = spec!({"pkg": "my-local-pkg/1.0.0"});
    rt.tmprepo.publish_spec(&spec).await.unwrap();
    let spec = spec!({"pkg": "my-local-pkg/1.0.0/BGSHW3CN"});
    rt.tmprepo
        .publish_package(
            &spec,
            vec![(api::Component::Run, empty_layer_digest())]
                .into_iter()
                .collect(),
        )
        .await
        .unwrap();

    let mut opt = Opt::try_parse_from(["ls", "--disable-repo", "origin"]).unwrap();
    opt.ls.run().await.unwrap();
    assert_eq!(opt.ls.output.vec.len(), 1);
    assert_eq!(opt.ls.output.vec.get(0).unwrap(), "my-local-pkg");
}

#[tokio::test]
async fn test_ls_succeeds_for_package_with_no_version_spec() {
    let mut rt = spfs_runtime().await;
    let remote_repo = spfsrepo().await;

    rt.add_remote_repo(
        "origin",
        Remote::Address(RemoteAddress {
            address: remote_repo.address().clone(),
        }),
    )
    .unwrap();

    // Publish a package (with a build) but no "version spec"
    let spec = spec!({"pkg": "my-pkg/1.0.0/BGSHW3CN"});
    rt.tmprepo
        .publish_package(
            &spec,
            vec![(api::Component::Run, empty_layer_digest())]
                .into_iter()
                .collect(),
        )
        .await
        .unwrap();

    let mut opt = Opt::try_parse_from(["ls", "my-pkg"]).unwrap();
    opt.ls.run().await.unwrap();
    assert_eq!(
        opt.ls.output.warnings.len(),
        0,
        "expected no warnings; got: {}",
        opt.ls.output.warnings[0]
    );
    assert_eq!(opt.ls.output.vec.len(), 1);
    assert_eq!(opt.ls.output.vec.get(0).unwrap(), "1.0.0");
}
