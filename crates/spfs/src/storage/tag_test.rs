// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

#[cfg(unix)]
use std::os::unix::fs::MetadataExt;

use chrono::prelude::*;
use futures::TryStreamExt;
use relative_path::RelativePathBuf;
use rstest::rstest;
use tokio_stream::StreamExt;

use crate::fixtures::*;
use crate::storage::fs::FsRepository;
use crate::storage::{EntryType, TagStorage};
use crate::{encoding, tracking, Result};

#[rstest]
#[case::fs(tmprepo("fs"))]
#[case::tar(tmprepo("tar"))]
#[cfg_attr(feature = "server", case::rpc(tmprepo("rpc")))]
#[tokio::test]
async fn test_tag_stream(
    #[case]
    #[future]
    tmprepo: TempRepo,
) {
    init_logging();
    let tmprepo = tmprepo.await;

    let digest1 = encoding::Hasher::new_sync().digest();
    let mut h = encoding::Hasher::new_sync();
    h.update(b"hello");
    let digest2 = h.digest();

    let base = crate::tracking::TagSpec::parse("hello/world").unwrap();
    let tag1 = tmprepo
        .push_tag(&base, &digest1)
        .await
        .expect("failed to push tag");
    assert_eq!(tmprepo.resolve_tag(&base).await.unwrap(), tag1);
    assert_eq!(
        tmprepo.resolve_tag(&base.with_version(0)).await.unwrap(),
        tag1
    );

    let tag2 = tmprepo
        .push_tag(&base, &digest2)
        .await
        .expect("failed to push tag");
    let _tag3 = tmprepo
        .push_tag(&base, &digest2)
        .await
        .expect("failed to push tag");
    assert_eq!(tmprepo.resolve_tag(&base).await.unwrap(), tag2);
    assert_eq!(
        tmprepo.resolve_tag(&base.with_version(0)).await.unwrap(),
        tag2
    );
    assert_eq!(
        tmprepo.resolve_tag(&base.with_version(1)).await.unwrap(),
        tag1
    );
    let found: crate::Result<Vec<_>> = tmprepo.find_tags(&digest2).collect().await;
    assert_eq!(found.unwrap(), vec![base.clone()]);
    let found: crate::Result<Vec<_>> = tmprepo.find_tags(&digest1).collect().await;
    assert_eq!(found.unwrap(), vec![base.with_version(1)]);
}

#[rstest]
#[case::fs(tmprepo("fs"))]
#[case::tar(tmprepo("tar"))]
#[cfg_attr(feature = "server", case::rpc(tmprepo("rpc")))]
#[tokio::test]
async fn test_tag_no_duplication(
    #[case]
    #[future]
    tmprepo: TempRepo,
) {
    init_logging();
    let tmprepo = tmprepo.await;

    let spec = tracking::TagSpec::parse("hello").unwrap();
    let tag1 = tmprepo
        .push_tag(&spec, &encoding::EMPTY_DIGEST.into())
        .await
        .unwrap();
    let tag2 = tmprepo
        .push_tag(&spec, &encoding::EMPTY_DIGEST.into())
        .await
        .unwrap();

    assert_eq!(tag1, tag2);

    tmprepo.insert_tag(&tag2).await.unwrap();
    tmprepo.insert_tag(&tag1).await.unwrap();

    assert_eq!(
        tmprepo
            .read_tag(&spec)
            .await
            .unwrap()
            // there's no count() for streams
            .fold(0, |c, _| c + 1)
            .await,
        1
    );
}

#[cfg(unix)]
#[rstest]
#[tokio::test]
async fn test_tag_permissions(tmpdir: tempfile::TempDir) {
    let storage = FsRepository::create(tmpdir.path().join("repo"))
        .await
        .unwrap();
    let spec = tracking::TagSpec::parse("hello").unwrap();
    storage
        .push_tag(&spec, &encoding::EMPTY_DIGEST.into())
        .await
        .unwrap();
    assert_eq!(
        tmpdir
            .path()
            .join("repo/tags/hello.tag")
            .metadata()
            .unwrap()
            .mode()
            & 0o666,
        0o666
    );
}

#[rstest]
#[case::fs(tmprepo("fs"))]
#[case::tar(tmprepo("tar"))]
#[cfg_attr(feature = "server", case::rpc(tmprepo("rpc")))]
#[tokio::test]
async fn test_ls_tags(
    #[case]
    #[future]
    tmprepo: TempRepo,
) {
    init_logging();
    let tmprepo = tmprepo.await;

    for tag in &[
        "spi/stable/my_tag",
        "spi/stable/other_tag",
        "spi/stable",
        "spi/latest/my_tag",
    ] {
        let spec = tracking::TagSpec::parse(tag).unwrap();
        tmprepo
            .push_tag(&spec, &encoding::EMPTY_DIGEST.into())
            .await
            .unwrap();
    }

    let mut tags: Vec<_> = tmprepo
        .ls_tags(&RelativePathBuf::from("/"))
        .collect::<Result<Vec<_>>>()
        .await
        .unwrap();
    assert_eq!(tags, vec![EntryType::Folder("spi".to_string())]);
    tags = tmprepo
        .ls_tags(&RelativePathBuf::from("/spi"))
        .collect::<Result<Vec<_>>>()
        .await
        .unwrap();
    tags.sort();
    assert_eq!(
        tags,
        vec![
            EntryType::Folder("latest".to_string()),
            EntryType::Folder("stable".to_string()),
            EntryType::Tag("stable".to_string()),
        ]
    );
    tags = tmprepo
        .ls_tags(&RelativePathBuf::from("spi/stable"))
        .collect::<Result<Vec<_>>>()
        .await
        .unwrap();
    tags.sort();
    assert_eq!(
        tags,
        vec![
            EntryType::Tag("my_tag".to_string()),
            EntryType::Tag("other_tag".to_string())
        ]
    );
}

#[rstest]
#[case::fs(tmprepo("fs"))]
#[case::tar(tmprepo("tar"))]
#[cfg_attr(feature = "server", case::rpc(tmprepo("rpc")))]
#[tokio::test]
async fn test_tag_ordering(
    #[case]
    #[future]
    tmprepo: TempRepo,
) {
    init_logging();
    let tmprepo = tmprepo.await;
    let tag_name = "spi/stable/my_tag";

    let spec = tracking::TagSpec::parse(tag_name).unwrap();
    let tag = tracking::Tag::new(spec.org(), spec.name(), encoding::EMPTY_DIGEST.into()).unwrap();

    for (i, utc) in [
        Utc.with_ymd_and_hms(1977, 5, 25, 0, 0, 0).unwrap(),
        Utc.with_ymd_and_hms(1977, 5, 25, 2, 0, 0).unwrap(),
        Utc.with_ymd_and_hms(1977, 5, 25, 4, 0, 0).unwrap(),
        Utc.with_ymd_and_hms(1977, 5, 25, 12, 0, 0).unwrap(),
        Utc.with_ymd_and_hms(1977, 5, 25, 4, 0, 0).unwrap(),
    ]
    .iter()
    .enumerate()
    {
        let mut tag = tag.clone();
        tag.time = *utc;
        if i == 4 {
            tag.target = encoding::NULL_DIGEST.into();
        }
        tmprepo.insert_tag(&tag).await.unwrap();
        println!("\n");
    }

    let tags: Vec<_> = tmprepo
        .read_tag(&spec)
        .await
        .unwrap()
        .try_collect()
        .await
        .unwrap();
    assert_eq!(tags.len(), 5);

    let mut prev_tag = tags.first().unwrap();
    for (i, tag) in tags.iter().enumerate() {
        if i == 0 {
            continue;
        }
        if tag.time == prev_tag.time {
            assert!(prev_tag.target > tag.target);
        } else {
            assert!(prev_tag.time > tag.time);
        }
        prev_tag = tag;
    }
}

#[rstest]
#[case::fs(tmprepo("fs"))]
#[case::tar(tmprepo("tar"))]
#[cfg_attr(feature = "server", case::rpc(tmprepo("rpc")))]
#[tokio::test]
async fn test_rm_tags(
    #[case]
    #[future]
    tmprepo: TempRepo,
) {
    init_logging();
    let tmprepo = tmprepo.await;

    for tag in &[
        "spi/stable/my_tag",
        "spi/stable/other_tag",
        "spi/latest/my_tag",
    ] {
        let spec = tracking::TagSpec::parse(tag).unwrap();
        tmprepo
            .push_tag(&spec, &encoding::EMPTY_DIGEST.into())
            .await
            .unwrap();
    }

    let mut tags: Vec<_> = tmprepo
        .ls_tags(&RelativePathBuf::from("/spi"))
        .collect::<Result<Vec<_>>>()
        .await
        .unwrap();
    tags.sort();
    assert_eq!(
        tags,
        vec![
            EntryType::Folder("latest".to_string()),
            EntryType::Folder("stable".to_string())
        ]
    );
    tmprepo
        .remove_tag_stream(&tracking::TagSpec::parse("spi/stable/my_tag").unwrap())
        .await
        .unwrap();
    tags = tmprepo
        .ls_tags(&RelativePathBuf::from("spi/stable"))
        .collect::<Result<Vec<_>>>()
        .await
        .unwrap();
    assert_eq!(tags, vec![EntryType::Tag("other_tag".to_string())]);
    tmprepo
        .remove_tag_stream(&tracking::TagSpec::parse("spi/stable/other_tag").unwrap())
        .await
        .unwrap();
    tags = tmprepo
        .ls_tags(&RelativePathBuf::from("spi"))
        .collect::<Result<Vec<_>>>()
        .await
        .unwrap();
    assert_eq!(
        tags,
        vec![EntryType::Folder("latest".to_string())],
        "should remove empty tag folders during cleanup"
    );

    let res = tmprepo
        .remove_tag_stream(&tracking::TagSpec::parse("spi/stable/other_tag").unwrap())
        .await;
    assert!(
        matches!(res, Err(crate::Error::UnknownReference(_))),
        "should fail to remove a removed tag, got {res:?}"
    );
}

#[rstest]
#[case::fs(tmprepo("fs"))]
#[tokio::test]
async fn test_tag_in_namespace(
    #[case]
    #[future]
    tmprepo: TempRepo,
) {
    init_logging();
    let tmprepo = tmprepo.await;

    let namespace_name = "test-namespace";
    let namespaced_repo = tmprepo.with_tag_namespace(namespace_name).await;

    // Create a tag in the namespaced repo.
    let tag_name = "a-tag";
    let spec = tracking::TagSpec::parse(tag_name).unwrap();
    namespaced_repo
        .push_tag(&spec, &encoding::EMPTY_DIGEST.into())
        .await
        .unwrap();

    // Listing the tags in the namespaced repo contains the tag we made.
    let tags: Vec<_> = namespaced_repo
        .ls_tags(&RelativePathBuf::from("/"))
        .collect::<Result<Vec<_>>>()
        .await
        .unwrap();
    assert_eq!(tags, vec![EntryType::Tag(tag_name.to_string())]);

    // Listing the tags in the non-namespaced repo contains [only] the
    // namespace.
    let tags: Vec<_> = tmprepo
        .ls_tags(&RelativePathBuf::from("/"))
        .collect::<Result<Vec<_>>>()
        .await
        .unwrap();
    assert_eq!(tags, vec![EntryType::Namespace(namespace_name.to_string())]);
}

#[rstest]
#[case::fs(tmprepo("fs"))]
#[tokio::test]
async fn test_tag_in_namespace_name_collision(
    #[case]
    #[future]
    tmprepo: TempRepo,
) {
    init_logging();
    let tmprepo = tmprepo.await;

    // It should be possible to have these distinct tags at the same time:
    //
    // | namespace | tag name    |
    // | --------- | ----------- |
    // | none      | foo/bar/baz |
    // | foo       | bar/baz     |
    // | foo/bar   | baz         |

    let repo_in_foo = tmprepo.with_tag_namespace("foo").await;
    let repo_in_foo_bar = tmprepo.with_tag_namespace("foo/bar").await;

    let foo_bar_baz = random_digest();
    let bar_baz = random_digest();
    let baz = random_digest();

    let spec_foo_bar_baz = tracking::TagSpec::parse("foo/bar/baz").unwrap();
    tmprepo
        .push_tag(&spec_foo_bar_baz, &foo_bar_baz)
        .await
        .unwrap();

    let spec_bar_baz = tracking::TagSpec::parse("bar/baz").unwrap();
    repo_in_foo.push_tag(&spec_bar_baz, &bar_baz).await.unwrap();

    let spec_baz = tracking::TagSpec::parse("baz").unwrap();
    repo_in_foo_bar.push_tag(&spec_baz, &baz).await.unwrap();

    // Now confirm these can be read back.

    let tag = repo_in_foo_bar.resolve_tag(&spec_baz).await.unwrap();
    assert_eq!(tag.target, baz);

    let tag = repo_in_foo.resolve_tag(&spec_bar_baz).await.unwrap();
    assert_eq!(tag.target, bar_baz);

    let tag = tmprepo.resolve_tag(&spec_foo_bar_baz).await.unwrap();
    assert_eq!(tag.target, foo_bar_baz);
}
