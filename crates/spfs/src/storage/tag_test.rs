// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use chrono::prelude::*;
use std::os::unix::fs::MetadataExt;

use futures::TryStreamExt;
use rstest::rstest;
use tokio_stream::StreamExt;

use crate::storage::{fs::FSRepository, EntryType, TagStorage};
use crate::{encoding, tracking, Result};
use relative_path::RelativePathBuf;

use crate::fixtures::*;

#[rstest(
    tmprepo,
    case::fs(tmprepo("fs")),
    case::tar(tmprepo("tar")),
    case::rpc(tmprepo("rpc"))
)]
#[tokio::test]
async fn test_tag_stream(#[future] tmprepo: TempRepo) {
    init_logging();
    let tmprepo = tmprepo.await;

    let digest1 = encoding::Hasher::default().digest();
    let mut h = encoding::Hasher::default();
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

#[rstest(
    tmprepo,
    case::fs(tmprepo("fs")),
    case::tar(tmprepo("tar")),
    case::rpc(tmprepo("rpc"))
)]
#[tokio::test]
async fn test_tag_no_duplication(#[future] tmprepo: TempRepo) {
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

#[rstest]
#[tokio::test]
async fn test_tag_permissions(tmpdir: tempdir::TempDir) {
    let storage = FSRepository::create(tmpdir.path().join("repo"))
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
            & 0o777,
        0o777
    );
}

#[rstest(
    tmprepo,
    case::fs(tmprepo("fs")),
    case::tar(tmprepo("tar")),
    case::rpc(tmprepo("rpc"))
)]
#[tokio::test]
async fn test_ls_tags(#[future] tmprepo: TempRepo) {
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

#[rstest(
    tmprepo,
    case::fs(tmprepo("fs")),
    case::tar(tmprepo("tar")),
    case::rpc(tmprepo("rpc"))
)]
#[tokio::test]
async fn test_tag_ordering(#[future] tmprepo: TempRepo) {
    init_logging();
    let tmprepo = tmprepo.await;
    let tag_name = "spi/stable/my_tag";

    let spec = tracking::TagSpec::parse(tag_name).unwrap();
    let tag = tracking::Tag::new(spec.org(), spec.name(), encoding::EMPTY_DIGEST.into()).unwrap();

    for (i, utc) in [
        Utc.ymd(1977, 5, 25).and_hms(0, 0, 0),
        Utc.ymd(1977, 5, 25).and_hms(2, 0, 0),
        Utc.ymd(1977, 5, 25).and_hms(4, 0, 0),
        Utc.ymd(1977, 5, 25).and_hms(12, 0, 0),
        Utc.ymd(1977, 5, 25).and_hms(4, 0, 0),
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

#[rstest(
    tmprepo,
    case::fs(tmprepo("fs")),
    case::tar(tmprepo("tar")),
    case::rpc(tmprepo("rpc"))
)]
#[tokio::test]
async fn test_tag_ordering(#[future] tmprepo: TempRepo) {
    init_logging();
    let tmprepo = tmprepo.await;
    let tag_name = "spi/stable/my_tag";

    let spec = tracking::TagSpec::parse(tag_name).unwrap();
    let tag = tracking::Tag::new(spec.org(), spec.name(), encoding::EMPTY_DIGEST.into()).unwrap();

    for (i, utc) in [
        Utc.ymd(1977, 5, 25).and_hms(0, 0, 0),
        Utc.ymd(1977, 5, 25).and_hms(2, 0, 0),
        Utc.ymd(1977, 5, 25).and_hms(4, 0, 0),
        Utc.ymd(1977, 5, 25).and_hms(12, 0, 0),
        Utc.ymd(1977, 5, 25).and_hms(4, 0, 0),
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

#[rstest(
    tmprepo,
    case::fs(tmprepo("fs")),
    case::tar(tmprepo("tar")),
    case::rpc(tmprepo("rpc"))
)]
#[tokio::test]
async fn test_rm_tags(#[future] tmprepo: TempRepo) {
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
}
