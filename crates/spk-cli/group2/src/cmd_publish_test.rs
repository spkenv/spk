// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use clap::Parser;
use futures::prelude::*;
use relative_path::RelativePathBuf;
use rstest::rstest;
use spfs::RemoteAddress;
use spfs::config::Remote;
use spfs::prelude::*;
use spfs::storage::EntryType;
use spk_schema::foundation::ident_component::Component;
use spk_schema::recipe;
use spk_solve::spec;
use spk_storage::RepositoryHandle;
use spk_storage::fixtures::*;

use super::{Publish, Run};

#[derive(Parser)]
struct Opt {
    #[clap(flatten)]
    publish: Publish,
}

/// Test when publishing a package that normalized tags are used when writing to
/// storage.
///
/// The publish command does not require that the number is written verbatim
/// compared to the version in the spec, as long as the version is considered
/// equal.
#[rstest]
#[tokio::test]
async fn test_publish_writes_with_normalized_version_tags(
    #[values("1", "1.0", "1.0.0", "1.0.0.0", "1.0.0.0.0")] version_to_create: &str,
    #[values("1", "1.0", "1.0.0", "1.0.0.0", "1.0.0.0.0")] version_to_publish: &str,
) {
    const VERSION_NORMALIZED: &str = "1.0.0";

    let mut rt = spfs_runtime().await;
    let remote_repo = spfsrepo().await;

    rt.add_remote_repo(
        "origin",
        Remote::Address(RemoteAddress {
            address: remote_repo.address().clone(),
        }),
    )
    .unwrap();

    let recipe = recipe!({"pkg": format!("my-local-pkg/{version_to_create}")});
    rt.tmprepo.publish_recipe(&recipe).await.unwrap();
    let spec = spec!({"pkg": format!("my-local-pkg/{version_to_create}/BGSHW3CN")});
    rt.tmprepo
        .publish_package(
            &spec,
            &vec![(Component::Run, empty_layer_digest())]
                .into_iter()
                .collect(),
        )
        .await
        .unwrap();

    // Confirm that the tags were created with the normalized version tag
    // strategy.
    for (tag_path, entry_type_filter) in [
        (
            "spk/spec/my-local-pkg",
            Box::new(|tag: &_| matches!(tag, Ok(EntryType::Tag(tag)) if tag == "1.0.0"))
                as Box<dyn for<'a> Fn(&'a Result<EntryType, spfs::Error>) -> bool>,
        ),
        (
            "spk/pkg/my-local-pkg",
            Box::new(|tag| matches!(tag, Ok(EntryType::Folder(tag)) if tag == "1.0.0")),
        ),
    ]
    .iter()
    {
        match &*rt.tmprepo {
            RepositoryHandle::SPFS(spfs) => {
                assert!(
                    spfs.ls_tags(&RelativePathBuf::from(tag_path))
                        .filter(|tag| { future::ready(entry_type_filter(tag)) })
                        .next()
                        .await
                        .is_some(),
                    "expected \"{tag_path}/1.0.0\" tag to be found"
                );
            }
            _ => panic!("expected SPFS"),
        }
    }

    match &*rt.tmprepo {
        RepositoryHandle::SPFS(spfs) => {
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
        _ => panic!("expected SPFS"),
    };

    let mut opt =
        Opt::try_parse_from(["publish", &format!("my-local-pkg/{version_to_publish}")]).unwrap();
    opt.publish.run().await.unwrap();

    // Confirm that the tags were created with the normalized version tag
    // strategy in the destination repository.
    for (tag_path, entry_type_filter) in [
        (
            "spk/spec/my-local-pkg",
            Box::new(|tag: &_| matches!(tag, Ok(EntryType::Tag(tag)) if tag == VERSION_NORMALIZED))
                as Box<dyn for<'a> Fn(&'a Result<EntryType, spfs::Error>) -> bool>,
        ),
        (
            "spk/pkg/my-local-pkg",
            Box::new(|tag| matches!(tag, Ok(EntryType::Folder(tag)) if tag == VERSION_NORMALIZED)),
        ),
    ]
    .iter()
    {
        match &*remote_repo.repo {
            RepositoryHandle::SPFS(spfs) => {
                assert!(
                    spfs.ls_tags(&RelativePathBuf::from(tag_path))
                        .filter(|tag| { future::ready(entry_type_filter(tag)) })
                        .next()
                        .await
                        .is_some(),
                    "expected \"{tag_path}/{VERSION_NORMALIZED}\" tag to be found"
                );
            }
            _ => panic!("expected SPFS"),
        }
    }

    match &*remote_repo.repo {
        RepositoryHandle::SPFS(spfs) => {
            assert!(
                spfs.ls_tags(&RelativePathBuf::from("spk/spec/my-local-pkg"))
                    .filter(|tag| {
                        future::ready(
                            matches!(tag, Ok(EntryType::Tag(tag)) if tag == VERSION_NORMALIZED),
                        )
                    })
                    .next()
                    .await
                    .is_some(),
                "expected \"{VERSION_NORMALIZED}\" tag to be found"
            );
        }
        _ => panic!("expected SPFS"),
    }
}
