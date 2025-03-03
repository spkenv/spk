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
use spk_schema::ident_ops::VerbatimTagStrategy;
use spk_schema::recipe;
use spk_solve::spec;
use spk_storage::RepositoryHandle;
use spk_storage::fixtures::*;

use super::{Remove, Run};

#[derive(Parser)]
struct Opt {
    #[clap(flatten)]
    remove: Remove,
}

/// When the legacy-spk-version-tags feature is enabled, and when a package
/// is published with a non-normalized version tag, `spk rm` is expected to
/// successfully delete the package.
#[rstest]
#[tokio::test]
async fn test_remove_succeeds_for_package_saved_with_legacy_version_tag(
    #[values("1", "1.0", "1.0.0", "1.0.0.0", "1.0.0.0.0")] version_to_publish: &str,
    #[values("1", "1.0", "1.0.0", "1.0.0.0", "1.0.0.0.0")] version_to_delete: &str,
) {
    let mut rt = spfs_runtime_with_tag_strategy::<VerbatimTagStrategy>().await;
    let remote_repo = spfsrepo().await;

    rt.add_remote_repo(
        "origin",
        Remote::Address(RemoteAddress {
            address: remote_repo.address().clone(),
        }),
    )
    .unwrap();

    let recipe = recipe!({"pkg": format!("my-local-pkg/{version_to_publish}")});
    rt.tmprepo.publish_recipe(&recipe).await.unwrap();
    let spec = spec!({"pkg": format!("my-local-pkg/{version_to_publish}/BGSHW3CN")});
    rt.tmprepo
        .publish_package(
            &spec,
            &vec![(Component::Run, empty_layer_digest())]
                .into_iter()
                .collect(),
        )
        .await
        .unwrap();

    // Confirm that the tags were created with the legacy version tag strategy.
    for (tag_path, entry_type_filter) in [
        (
            "spk/spec/my-local-pkg",
            Box::new(|tag: &_| matches!(tag, Ok(EntryType::Tag(tag)) if tag == version_to_publish))
                as Box<dyn for<'a> Fn(&'a Result<EntryType, spfs::Error>) -> bool>,
        ),
        (
            "spk/pkg/my-local-pkg",
            Box::new(|tag| matches!(tag, Ok(EntryType::Folder(tag)) if tag == version_to_publish)),
        ),
    ]
    .iter()
    {
        match &*rt.tmprepo {
            RepositoryHandle::SPFSWithVerbatimTags(spfs) => {
                assert!(
                    spfs.ls_tags(&RelativePathBuf::from(tag_path))
                        .filter(|tag| { future::ready(entry_type_filter(tag)) })
                        .next()
                        .await
                        .is_some(),
                    "expected \"{tag_path}/{version_to_publish}\" tag to be found"
                );
            }
            _ => panic!("expected SPFSWithVerbatimTags"),
        }
    }

    match &*rt.tmprepo {
        RepositoryHandle::SPFSWithVerbatimTags(spfs) => {
            assert!(
                spfs.ls_tags(&RelativePathBuf::from("spk/spec/my-local-pkg"))
                    .filter(|tag| {
                        future::ready(
                            matches!(tag, Ok(EntryType::Tag(tag)) if tag == version_to_publish),
                        )
                    })
                    .next()
                    .await
                    .is_some(),
                "expected \"{version_to_publish}\" tag to be found"
            );
        }
        _ => panic!("expected SPFSWithVerbatimTags"),
    }

    let mut opt = Opt::try_parse_from([
        "remove",
        "--legacy-spk-version-tags",
        "--yes",
        &format!("my-local-pkg/{version_to_delete}"),
    ])
    .unwrap();
    opt.remove.run().await.unwrap();

    // Confirm that the tags are now gone.
    for (tag_path, entry_type_filter) in [
        (
            "spk/spec/my-local-pkg",
            Box::new(|tag: &_| matches!(tag, Ok(EntryType::Tag(tag)) if tag == version_to_publish))
                as Box<dyn for<'a> Fn(&'a Result<EntryType, spfs::Error>) -> bool>,
        ),
        (
            "spk/pkg/my-local-pkg",
            Box::new(|tag| matches!(tag, Ok(EntryType::Folder(tag)) if tag == version_to_publish)),
        ),
    ]
    .iter()
    {
        match &*rt.tmprepo {
            RepositoryHandle::SPFSWithVerbatimTags(spfs) => {
                assert!(
                    spfs.ls_tags(&RelativePathBuf::from(tag_path))
                        .filter(|tag| { future::ready(entry_type_filter(tag)) })
                        .next()
                        .await
                        .is_none(),
                    "expected \"{tag_path}/{version_to_publish}\" tag to be gone"
                );
            }
            _ => panic!("expected SPFSWithVerbatimTags"),
        }
    }
}
