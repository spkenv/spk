// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::os::unix::fs::MetadataExt;

use rstest::rstest;
use tokio_stream::StreamExt;

use crate::storage::{fs::FSRepository, TagStorage};
use crate::{encoding, tracking};
use relative_path::RelativePathBuf;

fixtures!();

#[rstest]
fn test_tag_stream(tmpdir: tempdir::TempDir) {
    let _guard = init_logging();

    let mut storage = FSRepository::create(tmpdir.path()).expect("failed to create repo");

    let digest1 = encoding::Hasher::default().digest();
    let mut h = encoding::Hasher::default();
    h.update(b"hello");
    let digest2 = h.digest();

    let base = crate::tracking::TagSpec::parse("hello/world").unwrap();
    let tag1 = storage
        .push_tag(&base, &digest1)
        .expect("failed to push tag");
    assert_eq!(storage.resolve_tag(&base).unwrap(), tag1);
    assert_eq!(storage.resolve_tag(&base.with_version(0)).unwrap(), tag1);

    let tag2 = storage
        .push_tag(&base, &digest2)
        .expect("failed to push tag");
    let _tag3 = storage
        .push_tag(&base, &digest2)
        .expect("failed to push tag");
    assert_eq!(storage.resolve_tag(&base).unwrap(), tag2);
    assert_eq!(storage.resolve_tag(&base.with_version(0)).unwrap(), tag2);
    assert_eq!(storage.resolve_tag(&base.with_version(1)).unwrap(), tag1);
    let found: crate::Result<Vec<_>> = storage.find_tags(&digest2).collect();
    assert_eq!(found.unwrap(), vec![base.clone()]);
    let found: crate::Result<Vec<_>> = storage.find_tags(&digest1).collect();
    assert_eq!(found.unwrap(), vec![base.with_version(1)]);
}

#[rstest]
fn test_tag_no_duplication(tmpdir: tempdir::TempDir) {
    let _guard = init_logging();

    let mut storage = FSRepository::create(tmpdir.path().join("tags")).unwrap();
    let spec = tracking::TagSpec::parse("hello").unwrap();
    let tag1 = storage
        .push_tag(&spec, &encoding::EMPTY_DIGEST.into())
        .unwrap();
    let tag2 = storage
        .push_tag(&spec, &encoding::EMPTY_DIGEST.into())
        .unwrap();

    assert_eq!(tag1, tag2);

    assert_eq!(storage.read_tag(&spec).unwrap().count(), 1);
}

#[rstest]
fn test_tag_permissions(tmpdir: tempdir::TempDir) {
    let mut storage = FSRepository::create(tmpdir.path().join("repo")).unwrap();
    let spec = tracking::TagSpec::parse("hello").unwrap();
    storage
        .push_tag(&spec, &encoding::EMPTY_DIGEST.into())
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

#[rstest]
#[tokio::test]
async fn test_ls_tags(tmpdir: tempdir::TempDir) {
    let _guard = init_logging();

    let mut storage = FSRepository::create(tmpdir.path().join("tags")).unwrap();
    for tag in &[
        "spi/stable/my_tag",
        "spi/stable/other_tag",
        "spi/stable",
        "spi/latest/my_tag",
    ] {
        let spec = tracking::TagSpec::parse(tag).unwrap();
        storage
            .push_tag(&spec, &encoding::EMPTY_DIGEST.into())
            .unwrap();
    }

    let mut tags: Vec<_> = storage
        .ls_tags(&RelativePathBuf::from("/"))
        .unwrap()
        .collect()
        .await;
    assert_eq!(tags, vec!["spi/".to_string()]);
    tags = storage
        .ls_tags(&RelativePathBuf::from("/spi"))
        .unwrap()
        .collect()
        .await;
    tags.sort();
    assert_eq!(
        tags,
        vec![
            "latest/".to_string(),
            "stable".to_string(),
            "stable/".to_string()
        ]
    );
    tags = storage
        .ls_tags(&RelativePathBuf::from("spi/stable"))
        .unwrap()
        .collect()
        .await;
    tags.sort();
    assert_eq!(tags, vec!["my_tag".to_string(), "other_tag".to_string()]);
}

#[rstest]
#[tokio::test]
async fn test_rm_tags(tmpdir: tempdir::TempDir) {
    let _guard = init_logging();

    let mut storage = FSRepository::create(tmpdir.path().join("tags")).unwrap();
    for tag in &[
        "spi/stable/my_tag",
        "spi/stable/other_tag",
        "spi/latest/my_tag",
    ] {
        let spec = tracking::TagSpec::parse(tag).unwrap();
        storage
            .push_tag(&spec, &encoding::EMPTY_DIGEST.into())
            .unwrap();
    }

    let mut tags: Vec<_> = storage
        .ls_tags(&RelativePathBuf::from("/spi"))
        .unwrap()
        .collect()
        .await;
    tags.sort();
    assert_eq!(tags, vec!["latest/", "stable/"]);
    storage
        .remove_tag_stream(&tracking::TagSpec::parse("spi/stable/my_tag").unwrap())
        .unwrap();
    tags = storage
        .ls_tags(&RelativePathBuf::from("spi/stable"))
        .unwrap()
        .collect()
        .await;
    assert_eq!(tags, vec!["other_tag"]);
    storage
        .remove_tag_stream(&tracking::TagSpec::parse("spi/stable/other_tag").unwrap())
        .unwrap();
    tags = storage
        .ls_tags(&RelativePathBuf::from("spi"))
        .unwrap()
        .collect()
        .await;
    assert_eq!(
        tags,
        vec!["latest/"],
        "should remove empty tag folders during cleanup"
    );
}
