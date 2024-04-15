// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use clap::Parser;
use futures::prelude::*;
use relative_path::RelativePathBuf;
use spfs::config::Remote;
use spfs::prelude::*;
use spfs::storage::EntryType;
use spfs::RemoteAddress;
use spk_schema::foundation::ident_component::Component;
use spk_schema::ident_ops::VerbatimTagStrategy;
use spk_schema::name::OptName;
use spk_schema::recipe;
use spk_solve::spec;
use spk_storage::fixtures::*;
use spk_storage::RepositoryHandle;

use super::{Ls, Output, Run};
use crate::cmd_ls::HOST_OPTIONS;

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

/// `spk ls --no-host` is expected to list all packages in the configured
/// remote repositories.
#[tokio::test]
async fn test_ls_shows_remote_packages_with_no_host() {
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

    let mut opt = Opt::try_parse_from(["ls", "--no-host"]).unwrap();
    opt.ls.run().await.unwrap();
    assert_ne!(opt.ls.output.vec.len(), 0);
}

/// `spk ls` is expected to list packages in the configured remote
/// repositories that match the default filter for the current host
#[tokio::test]
async fn test_ls_shows_remote_packages_with_host_default_filter() {
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

    let recipe = recipe!({"pkg": "my-pkg/1.0.0"});
    remote_repo.publish_recipe(&recipe).await.unwrap();
    let host_options = HOST_OPTIONS.get().unwrap();
    let os_id = host_options.get(OptName::distro()).unwrap();
    let spec = spec!({"pkg": "my-pkg/1.0.0/BGSHW3CN",
    "build": {
        "options":
         [
             {"var": format!("{}/{}", OptName::distro(), host_options.get(OptName::distro()).unwrap()) },
             {"var": format!("{}/{}", OptName::os(), host_options.get(OptName::os()).unwrap()) },
             {"var": format!("{}/{}", OptName::arch(), host_options.get(OptName::arch()).unwrap()) },
             {"var": format!("{}/{}", os_id, host_options.get(os_id).unwrap()) }
         ]
    }});
    remote_repo
        .publish_package(
            &spec,
            &vec![(Component::Run, empty_layer_digest())]
                .into_iter()
                .collect(),
        )
        .await
        .unwrap();

    let mut opt = Opt::try_parse_from(["ls", "--host"]).unwrap();
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

    let recipe = recipe!({"pkg": "my-remote-pkg/1.0.0"});
    remote_repo.publish_recipe(&recipe).await.unwrap();
    let spec = spec!({"pkg": "my-remote-pkg/1.0.0/BGSHW3CN"});
    remote_repo
        .publish_package(
            &spec,
            &vec![(Component::Run, empty_layer_digest())]
                .into_iter()
                .collect(),
        )
        .await
        .unwrap();

    let recipe = recipe!({"pkg": "my-local-pkg/1.0.0"});
    rt.tmprepo.publish_recipe(&recipe).await.unwrap();
    let spec = spec!({"pkg": "my-local-pkg/1.0.0/BGSHW3CN"});
    rt.tmprepo
        .publish_package(
            &spec,
            &vec![(Component::Run, empty_layer_digest())]
                .into_iter()
                .collect(),
        )
        .await
        .unwrap();

    let mut opt = Opt::try_parse_from(["ls", "--no-host"]).unwrap();
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

    let recipe = recipe!({"pkg": "my-remote-pkg/1.0.0"});
    remote_repo.publish_recipe(&recipe).await.unwrap();
    let spec = spec!({"pkg": "my-remote-pkg/1.0.0/BGSHW3CN"});
    remote_repo
        .publish_package(
            &spec,
            &vec![(Component::Run, empty_layer_digest())]
                .into_iter()
                .collect(),
        )
        .await
        .unwrap();

    let recipe = recipe!({"pkg": "my-local-pkg/1.0.0"});
    rt.tmprepo.publish_recipe(&recipe).await.unwrap();
    let spec = spec!({"pkg": "my-local-pkg/1.0.0/BGSHW3CN"});
    rt.tmprepo
        .publish_package(
            &spec,
            &vec![(Component::Run, empty_layer_digest())]
                .into_iter()
                .collect(),
        )
        .await
        .unwrap();

    let mut opt = Opt::try_parse_from(["ls", "-L", "--no-host"]).unwrap();
    opt.ls.run().await.unwrap();
    assert_eq!(opt.ls.output.vec.len(), 1);
    assert_eq!(opt.ls.output.vec.first().unwrap(), "my-local-pkg");
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

    let recipe = recipe!({"pkg": "my-remote-pkg/1.0.0"});
    remote_repo.publish_recipe(&recipe).await.unwrap();
    let spec = spec!({"pkg": "my-remote-pkg/1.0.0/BGSHW3CN"});
    remote_repo
        .publish_package(
            &spec,
            &vec![(Component::Run, empty_layer_digest())]
                .into_iter()
                .collect(),
        )
        .await
        .unwrap();

    let recipe = recipe!({"pkg": "my-local-pkg/1.0.0"});
    rt.tmprepo.publish_recipe(&recipe).await.unwrap();
    let spec = spec!({"pkg": "my-local-pkg/1.0.0/BGSHW3CN"});
    rt.tmprepo
        .publish_package(
            &spec,
            &vec![(Component::Run, empty_layer_digest())]
                .into_iter()
                .collect(),
        )
        .await
        .unwrap();

    let mut opt = Opt::try_parse_from(["ls", "-r", "origin", "--no-host"]).unwrap();
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

    let recipe = recipe!({"pkg": "my-remote-pkg/1.0.0"});
    remote_repo.publish_recipe(&recipe).await.unwrap();
    let spec = spec!({"pkg": "my-remote-pkg/1.0.0/BGSHW3CN"});
    remote_repo
        .publish_package(
            &spec,
            &vec![(Component::Run, empty_layer_digest())]
                .into_iter()
                .collect(),
        )
        .await
        .unwrap();

    let recipe = recipe!({"pkg": "my-local-pkg/1.0.0"});
    rt.tmprepo.publish_recipe(&recipe).await.unwrap();
    let spec = spec!({"pkg": "my-local-pkg/1.0.0/BGSHW3CN"});
    rt.tmprepo
        .publish_package(
            &spec,
            &vec![(Component::Run, empty_layer_digest())]
                .into_iter()
                .collect(),
        )
        .await
        .unwrap();

    let mut opt = Opt::try_parse_from(["ls", "--no-local-repo", "--no-host"]).unwrap();
    opt.ls.run().await.unwrap();
    assert_eq!(opt.ls.output.vec.len(), 1);
    assert_eq!(opt.ls.output.vec.first().unwrap(), "my-remote-pkg");
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

    let recipe = recipe!({"pkg": "my-remote-pkg/1.0.0"});
    remote_repo.publish_recipe(&recipe).await.unwrap();
    let spec = spec!({"pkg": "my-remote-pkg/1.0.0/BGSHW3CN"});
    remote_repo
        .publish_package(
            &spec,
            &vec![(Component::Run, empty_layer_digest())]
                .into_iter()
                .collect(),
        )
        .await
        .unwrap();

    let recipe = recipe!({"pkg": "my-local-pkg/1.0.0"});
    rt.tmprepo.publish_recipe(&recipe).await.unwrap();
    let spec = spec!({"pkg": "my-local-pkg/1.0.0/BGSHW3CN"});
    rt.tmprepo
        .publish_package(
            &spec,
            &vec![(Component::Run, empty_layer_digest())]
                .into_iter()
                .collect(),
        )
        .await
        .unwrap();

    let mut opt = Opt::try_parse_from(["ls", "--disable-repo", "origin", "--no-host"]).unwrap();
    opt.ls.run().await.unwrap();
    assert_eq!(opt.ls.output.vec.len(), 1);
    assert_eq!(opt.ls.output.vec.first().unwrap(), "my-local-pkg");
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
            &vec![(Component::Run, empty_layer_digest())]
                .into_iter()
                .collect(),
        )
        .await
        .unwrap();

    let mut opt = Opt::try_parse_from(["ls", "my-pkg", "--no-host"]).unwrap();
    opt.ls.run().await.unwrap();
    assert_eq!(
        opt.ls.output.warnings.len(),
        0,
        "expected no warnings; got: {}",
        opt.ls.output.warnings[0]
    );
    assert_eq!(opt.ls.output.vec.len(), 1);
    assert_eq!(opt.ls.output.vec.first().unwrap(), "1.0.0");
}

#[tokio::test]
async fn test_ls_hides_deprecated_version() {
    let mut rt = spfs_runtime().await;
    let remote_repo = spfsrepo().await;

    rt.add_remote_repo(
        "origin",
        Remote::Address(RemoteAddress {
            address: remote_repo.address().clone(),
        }),
    )
    .unwrap();

    let spec = spec!({
        "pkg": "my-pkg/1.0.0/BGSHW3CN",
        "deprecated": true,
    });
    rt.tmprepo
        .publish_package(
            &spec,
            &vec![(Component::Run, empty_layer_digest())]
                .into_iter()
                .collect(),
        )
        .await
        .unwrap();

    // `ls` without showing deprecated
    let mut opt = Opt::try_parse_from(["ls", "my-pkg", "--no-host"]).unwrap();
    opt.ls.run().await.unwrap();
    assert_eq!(
        opt.ls.output.warnings.len(),
        0,
        "expected no warnings; got: {}",
        opt.ls.output.warnings[0]
    );
    assert_eq!(
        opt.ls.output.vec.len(),
        0,
        "expected no version listed; got: {}",
        opt.ls.output.vec[0]
    );

    // `ls` with showing deprecated
    let mut opt = Opt::try_parse_from(["ls", "--deprecated", "my-pkg", "--no-host"]).unwrap();
    opt.ls.run().await.unwrap();
    assert_eq!(
        opt.ls.output.warnings.len(),
        0,
        "expected no warnings; got: {}",
        opt.ls.output.warnings[0]
    );
    assert_eq!(opt.ls.output.vec.len(), 1);
    assert!(opt.ls.output.vec.first().unwrap().contains("1.0.0"));
    assert!(opt.ls.output.vec.first().unwrap().contains("DEPRECATED"));
}

#[tokio::test]
async fn test_ls_shows_partially_deprecated_version() {
    let mut rt = spfs_runtime().await;
    let remote_repo = spfsrepo().await;

    rt.add_remote_repo(
        "origin",
        Remote::Address(RemoteAddress {
            address: remote_repo.address().clone(),
        }),
    )
    .unwrap();

    // Publish two specs; one deprecated and one not.

    let spec = spec!({
        "pkg": "my-pkg/1.0.0/BGSHW3CN",
        "deprecated": true,
    });
    rt.tmprepo
        .publish_package(
            &spec,
            &vec![(Component::Run, empty_layer_digest())]
                .into_iter()
                .collect(),
        )
        .await
        .unwrap();

    let spec = spec!({"pkg": "my-pkg/1.0.0/CU7ZWOIF"});
    rt.tmprepo
        .publish_package(
            &spec,
            &vec![(Component::Run, empty_layer_digest())]
                .into_iter()
                .collect(),
        )
        .await
        .unwrap();

    // `ls` without showing deprecated
    let mut opt = Opt::try_parse_from(["ls", "my-pkg", "--no-host"]).unwrap();
    opt.ls.run().await.unwrap();
    assert_eq!(
        opt.ls.output.warnings.len(),
        0,
        "expected no warnings; got: {}",
        opt.ls.output.warnings[0]
    );
    // There is at least one non-deprecated build, so the version should be
    // listed.
    assert_eq!(opt.ls.output.vec.len(), 1);
    assert_eq!(opt.ls.output.vec.first().unwrap(), "1.0.0");

    // `ls` with showing deprecated
    let mut opt = Opt::try_parse_from(["ls", "--deprecated", "my-pkg", "--no-host"]).unwrap();
    opt.ls.run().await.unwrap();
    assert_eq!(
        opt.ls.output.warnings.len(),
        0,
        "expected no warnings; got: {}",
        opt.ls.output.warnings[0]
    );
    assert_eq!(opt.ls.output.vec.len(), 1);
    assert!(opt.ls.output.vec.first().unwrap().contains("1.0.0"));
    assert!(opt.ls.output.vec.first().unwrap().contains("partially"));
    assert!(opt.ls.output.vec.first().unwrap().contains("DEPRECATED"));
}

/// When the legacy-spk-version-tags feature is enabled, and when a package
/// is published with a non-normalized version tag, `spk ls` is expected to
/// list the package.
#[tokio::test]
async fn test_ls_succeeds_for_package_saved_with_legacy_version_tag() {
    let mut rt = spfs_runtime_with_tag_strategy::<VerbatimTagStrategy>().await;
    let remote_repo = spfsrepo().await;

    rt.add_remote_repo(
        "origin",
        Remote::Address(RemoteAddress {
            address: remote_repo.address().clone(),
        }),
    )
    .unwrap();

    let recipe = recipe!({"pkg": "my-local-pkg/1.0.0"});
    rt.tmprepo.publish_recipe(&recipe).await.unwrap();
    let spec = spec!({"pkg": "my-local-pkg/1.0.0/BGSHW3CN"});
    rt.tmprepo
        .publish_package(
            &spec,
            &vec![(Component::Run, empty_layer_digest())]
                .into_iter()
                .collect(),
        )
        .await
        .unwrap();

    // Confirm that the tag was created with the legacy version tag strategy.
    match &*rt.tmprepo {
        RepositoryHandle::SPFSWithVerbatimTags(spfs) => {
            assert!(
                spfs.ls_tags(&RelativePathBuf::from("spk/spec/my-local-pkg"))
                    .filter(|tag| {
                        future::ready(matches!(tag, Ok(EntryType::Tag(tag)) if tag == "1.0.0"))
                    })
                    .next()
                    .await
                    .is_some(),
                "expected \"1.0.0\" tag to be found"
            );
        }
        _ => panic!("expected SPFSWithVerbatimTags"),
    }

    let mut opt = Opt::try_parse_from([
        "ls",
        "--legacy-spk-version-tags",
        "my-local-pkg",
        "--no-host",
    ])
    .unwrap();
    opt.ls.run().await.unwrap();
    assert_eq!(opt.ls.output.vec.len(), 1);
}
