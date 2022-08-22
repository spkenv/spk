// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use rstest::rstest;

use crate::fixtures::*;
use crate::prelude::*;

#[rstest]
#[tokio::test]
async fn test_proxy_payload_read_through(tmpdir: tempfile::TempDir) {
    init_logging();

    let primary = crate::storage::fs::FSRepository::create(tmpdir.path().join("primary"))
        .await
        .unwrap();
    let secondary = crate::storage::fs::FSRepository::create(tmpdir.path().join("secondary"))
        .await
        .unwrap();

    let digest = secondary
        .commit_blob(Box::pin(b"some data".as_slice()))
        .await
        .unwrap();

    let proxy = super::ProxyRepository {
        primary: primary.into(),
        secondary: vec![secondary.into()],
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

    let primary = crate::storage::fs::FSRepository::create(tmpdir.path().join("primary"))
        .await
        .unwrap();
    let secondary = crate::storage::fs::FSRepository::create(tmpdir.path().join("secondary"))
        .await
        .unwrap();

    let payload = secondary
        .commit_blob(Box::pin(b"some data".as_slice()))
        .await
        .unwrap();

    let proxy = super::ProxyRepository {
        primary: primary.into(),
        secondary: vec![secondary.into()],
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

    let primary = crate::storage::fs::FSRepository::create(tmpdir.path().join("primary"))
        .await
        .unwrap();
    let secondary = crate::storage::fs::FSRepository::create(tmpdir.path().join("secondary"))
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
    };

    proxy
        .resolve_tag(&tag_spec)
        .await
        .expect("tag should be resolvable via the secondary repo");
}
