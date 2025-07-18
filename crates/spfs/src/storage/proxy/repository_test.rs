// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::collections::HashSet;

use futures::StreamExt;
use rstest::rstest;

use crate::config::default_proxy_repo_include_secondary_tags;
use crate::fixtures::*;
use crate::prelude::*;
use crate::storage::proxy::repository::RelativePath;

#[rstest]
#[tokio::test]
async fn test_proxy_payload_read_through(tmpdir: tempfile::TempDir) {
    init_logging();

    let primary = crate::storage::fs::MaybeOpenFsRepository::create(tmpdir.path().join("primary"))
        .await
        .unwrap();
    let secondary =
        crate::storage::fs::MaybeOpenFsRepository::create(tmpdir.path().join("secondary"))
            .await
            .unwrap();

    let digest = secondary
        .commit_blob(Box::pin(b"some data".as_slice()))
        .await
        .unwrap();

    let proxy = super::ProxyRepository {
        primary: primary.into(),
        secondary: vec![secondary.into()],
        include_secondary_tags: default_proxy_repo_include_secondary_tags(),
    };

    proxy
        .open_payload(digest)
        .await
        .expect("payload should be loadable via the secondary");
}

#[rstest]
#[tokio::test]
async fn test_proxy_object_read_through(tmpdir: tempfile::TempDir) {
    init_logging();

    let primary = crate::storage::fs::MaybeOpenFsRepository::create(tmpdir.path().join("primary"))
        .await
        .unwrap();
    let secondary =
        crate::storage::fs::MaybeOpenFsRepository::create(tmpdir.path().join("secondary"))
            .await
            .unwrap();

    let payload = secondary
        .commit_blob(Box::pin(b"some data".as_slice()))
        .await
        .unwrap();

    let proxy = super::ProxyRepository {
        primary: primary.into(),
        secondary: vec![secondary.into()],
        include_secondary_tags: default_proxy_repo_include_secondary_tags(),
    };

    proxy
        .read_object(payload)
        .await
        .expect("object should be loadable via the secondary repo");
}

#[rstest]
#[tokio::test]
async fn test_proxy_tag_read_through(tmpdir: tempfile::TempDir) {
    init_logging();

    let primary = crate::storage::fs::MaybeOpenFsRepository::create(tmpdir.path().join("primary"))
        .await
        .unwrap();
    let secondary =
        crate::storage::fs::MaybeOpenFsRepository::create(tmpdir.path().join("secondary"))
            .await
            .unwrap();

    let payload = secondary
        .commit_blob(Box::pin(b"some data".as_slice()))
        .await
        .unwrap();
    let tag_spec = crate::tracking::TagSpec::parse("spfs-test/proxy-read-through").unwrap();
    secondary.push_tag(&tag_spec, &payload).await.unwrap();

    let proxy = super::ProxyRepository {
        primary: primary.into(),
        secondary: vec![secondary.into()],
        include_secondary_tags: default_proxy_repo_include_secondary_tags(),
    };

    proxy
        .resolve_tag(&tag_spec)
        .await
        .expect("tag should be resolvable via the secondary repo");
}

#[rstest]
#[tokio::test]
async fn test_proxy_tag_ls(tmpdir: tempfile::TempDir) {
    init_logging();

    let primary = crate::storage::fs::MaybeOpenFsRepository::create(tmpdir.path().join("primary"))
        .await
        .unwrap();

    let payload1 = primary
        .commit_blob(Box::pin(b"some data".as_slice()))
        .await
        .unwrap();
    let tag_spec = crate::tracking::TagSpec::parse("spfs-test/proxy-read-through").unwrap();
    primary.push_tag(&tag_spec, &payload1).await.unwrap();

    let secondary =
        crate::storage::fs::MaybeOpenFsRepository::create(tmpdir.path().join("secondary"))
            .await
            .unwrap();

    let payload2 = secondary
        .commit_blob(Box::pin(b"some data".as_slice()))
        .await
        .unwrap();
    secondary.push_tag(&tag_spec, &payload2).await.unwrap();

    let proxy = super::ProxyRepository {
        primary: primary.into(),
        secondary: vec![secondary.into()],
        include_secondary_tags: default_proxy_repo_include_secondary_tags(),
    };

    let path = RelativePath::new("spfs-test");
    let mut tags = proxy.ls_tags(path);

    let mut seen = HashSet::new();
    while let Some(item) = tags.next().await {
        let tag = match item {
            Ok(t) => t,
            Err(err) => panic!("ls_tags errored: {err}"),
        };
        println!("found: {tag}");
        if !seen.insert(tag.to_string()) {
            panic!("duplicate tag found: '{tag}'. duplicates should not be returned");
        }
    }
}

#[rstest]
#[tokio::test]
async fn test_proxy_tag_ls_config_for_primary_only(tmpdir: tempfile::TempDir) {
    init_logging();

    let primary = crate::storage::fs::MaybeOpenFsRepository::create(tmpdir.path().join("primary"))
        .await
        .unwrap();

    let payload1 = primary
        .commit_blob(Box::pin(b"some data".as_slice()))
        .await
        .unwrap();
    let tag_spec = crate::tracking::TagSpec::parse("spfs-test/proxy-read-through").unwrap();
    primary.push_tag(&tag_spec, &payload1).await.unwrap();

    let secondary =
        crate::storage::fs::MaybeOpenFsRepository::create(tmpdir.path().join("secondary"))
            .await
            .unwrap();

    let payload2 = secondary
        .commit_blob(Box::pin(b"some data".as_slice()))
        .await
        .unwrap();
    let tag_spec2 = crate::tracking::TagSpec::parse("spfs-test/proxy-read-through2").unwrap();
    secondary.push_tag(&tag_spec2, &payload2).await.unwrap();

    let proxy = super::ProxyRepository {
        primary: primary.into(),
        secondary: vec![secondary.into()],
        // This is the configuration change for that sets up this test
        include_secondary_tags: false,
    };

    let path = RelativePath::new("spfs-test");
    let mut tags = proxy.ls_tags(path);

    let mut count = 0;

    let mut seen = HashSet::new();
    while let Some(item) = tags.next().await {
        let tag = match item {
            Ok(t) => t,
            Err(err) => panic!("ls_tags errored: {err}"),
        };
        println!("found: {tag}");
        if !seen.insert(tag.to_string()) {
            panic!("duplicate tag found: '{tag}'. duplicates should not be returned");
        } else {
            count += 1;
        }
    }

    assert_eq!(
        count, 1,
        "There should only be 1 tag in the primary when pass through to secondaries is disabled"
    );
}

#[rstest]
#[tokio::test]
async fn test_proxy_tag_find(tmpdir: tempfile::TempDir) {
    init_logging();

    let primary = crate::storage::fs::MaybeOpenFsRepository::create(tmpdir.path().join("primary"))
        .await
        .unwrap();

    let payload1 = primary
        .commit_blob(Box::pin(b"some data".as_slice()))
        .await
        .unwrap();
    let tag_spec = crate::tracking::TagSpec::parse("spfs-test/proxy-read-through").unwrap();
    primary.push_tag(&tag_spec, &payload1).await.unwrap();

    let secondary =
        crate::storage::fs::MaybeOpenFsRepository::create(tmpdir.path().join("secondary"))
            .await
            .unwrap();

    let payload2 = secondary
        .commit_blob(Box::pin(b"some data".as_slice()))
        .await
        .unwrap();
    secondary.push_tag(&tag_spec, &payload2).await.unwrap();

    let proxy = super::ProxyRepository {
        primary: primary.into(),
        secondary: vec![secondary.into()],
        include_secondary_tags: default_proxy_repo_include_secondary_tags(),
    };

    let mut tags = proxy.find_tags_in_namespace(None, &payload2);

    let mut seen = HashSet::new();
    while let Some(item) = tags.next().await {
        let tag = match item {
            Ok(t) => t,
            Err(err) => panic!("ls_tags errored: {err}"),
        };
        println!("found: {tag}");
        if !seen.insert(tag.to_string()) {
            panic!("duplicate tag found: '{tag}'. duplicates should not be returned");
        }
    }
}

#[rstest]
#[tokio::test]
async fn test_proxy_tag_find_for_primary_only(tmpdir: tempfile::TempDir) {
    init_logging();

    let primary = crate::storage::fs::MaybeOpenFsRepository::create(tmpdir.path().join("primary"))
        .await
        .unwrap();

    let payload1 = primary
        .commit_blob(Box::pin(b"some data".as_slice()))
        .await
        .unwrap();
    let tag_spec = crate::tracking::TagSpec::parse("spfs-test/proxy-read-through").unwrap();
    primary.push_tag(&tag_spec, &payload1).await.unwrap();

    let secondary =
        crate::storage::fs::MaybeOpenFsRepository::create(tmpdir.path().join("secondary"))
            .await
            .unwrap();

    let payload2 = secondary
        .commit_blob(Box::pin(b"some data".as_slice()))
        .await
        .unwrap();
    let tag_spec2 = crate::tracking::TagSpec::parse("spfs-test/proxy-read-through2").unwrap();
    secondary.push_tag(&tag_spec2, &payload2).await.unwrap();

    let proxy = super::ProxyRepository {
        primary: primary.into(),
        secondary: vec![secondary.into()],
        // This is the configuration change for that sets up this test
        include_secondary_tags: false,
    };

    let mut tags = proxy.find_tags_in_namespace(None, &payload2);

    let mut seen = HashSet::new();
    while let Some(item) = tags.next().await {
        let tag = match item {
            Ok(t) => t,
            Err(err) => panic!("ls_tags errored: {err} - but I think it should?"),
        };
        seen.insert(tag.to_string());
    }

    println!("seen: {seen:?}");
    assert!(seen.contains(&tag_spec.to_string()));
    assert!(!seen.contains(&tag_spec2.to_string()));
}

#[rstest]
#[tokio::test]
async fn test_proxy_tag_iter_streams(tmpdir: tempfile::TempDir) {
    init_logging();

    let primary = crate::storage::fs::MaybeOpenFsRepository::create(tmpdir.path().join("primary"))
        .await
        .unwrap();

    let payload1 = primary
        .commit_blob(Box::pin(b"some data".as_slice()))
        .await
        .unwrap();
    let tag_spec = crate::tracking::TagSpec::parse("spfs-test/proxy-read-through").unwrap();
    primary.push_tag(&tag_spec, &payload1).await.unwrap();

    let secondary =
        crate::storage::fs::MaybeOpenFsRepository::create(tmpdir.path().join("secondary"))
            .await
            .unwrap();

    let payload2 = secondary
        .commit_blob(Box::pin(b"some data".as_slice()))
        .await
        .unwrap();
    secondary.push_tag(&tag_spec, &payload2).await.unwrap();

    let proxy = super::ProxyRepository {
        primary: primary.into(),
        secondary: vec![secondary.into()],
        include_secondary_tags: default_proxy_repo_include_secondary_tags(),
    };

    let mut tags = proxy.iter_tag_streams_in_namespace(None);

    let mut seen = HashSet::new();
    while let Some(item) = tags.next().await {
        let tag = match item {
            Ok((t, _ts)) => t,
            Err(err) => panic!("ls_tags errored: {err}"),
        };
        println!("found: {tag}");
        if !seen.insert(tag.to_string()) {
            panic!("duplicate tag found: '{tag}'. duplicates should not be returned");
        }
    }
}

#[rstest]
#[tokio::test]
async fn test_proxy_tag_iter_streams_for_primary_only(tmpdir: tempfile::TempDir) {
    init_logging();

    let primary = crate::storage::fs::MaybeOpenFsRepository::create(tmpdir.path().join("primary"))
        .await
        .unwrap();

    let payload1 = primary
        .commit_blob(Box::pin(b"some data".as_slice()))
        .await
        .unwrap();
    let tag_spec = crate::tracking::TagSpec::parse("spfs-test/proxy-read-through").unwrap();
    primary.push_tag(&tag_spec, &payload1).await.unwrap();

    let secondary =
        crate::storage::fs::MaybeOpenFsRepository::create(tmpdir.path().join("secondary"))
            .await
            .unwrap();

    let payload2 = secondary
        .commit_blob(Box::pin(b"some data".as_slice()))
        .await
        .unwrap();
    let tag_spec2 = crate::tracking::TagSpec::parse("spfs-test/proxy-read-through2").unwrap();
    secondary.push_tag(&tag_spec2, &payload2).await.unwrap();

    let proxy = super::ProxyRepository {
        primary: primary.into(),
        secondary: vec![secondary.into()],
        // This is the configuration change for that sets up this test
        include_secondary_tags: false,
    };

    let mut tags = proxy.iter_tag_streams_in_namespace(None);

    let mut seen = HashSet::new();
    while let Some(item) = tags.next().await {
        let tag = match item {
            Ok((t, _ts)) => t,
            Err(err) => panic!("ls_tags errored: {err}"),
        };
        seen.insert(tag.to_string());
    }
    println!("seen: {seen:?}");
    assert!(seen.contains(&tag_spec.to_string()));
    assert!(!seen.contains(&tag_spec2.to_string()));
}
