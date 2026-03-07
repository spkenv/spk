// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::sync::Arc;

use rstest::rstest;

use crate::fixtures::*;
use crate::prelude::*;
use crate::storage::TryRenderStore;
use crate::storage::fs::{MaybeOpenFsRepository, MaybeRenderStore, RenderStore};

#[rstest]
#[tokio::test]
async fn test_proxy_payload_repair(tmpdir: tempfile::TempDir) {
    init_logging();

    let primary = Arc::new(
        crate::storage::fs::OpenFsRepository::<RenderStore>::create(tmpdir.path().join("primary"))
            .await
            .unwrap(),
    );
    let secondary = Arc::new(
        crate::storage::fs::OpenFsRepository::<RenderStore>::create(
            tmpdir.path().join("secondary"),
        )
        .await
        .unwrap(),
    );

    let digest = primary
        .commit_blob(Box::pin(b"some data".as_slice()))
        .await
        .unwrap();

    secondary
        .commit_blob(Box::pin(b"some data".as_slice()))
        .await
        .unwrap();

    // Delete the payload file from the primary repo.
    let payload_path = primary.payloads.build_digest_path(&digest);
    tokio::fs::remove_file(payload_path).await.unwrap();

    // Loading the payload from the primary should fail.
    let err = primary.open_payload(digest).await;
    assert!(err.is_err());

    // Loading the payload through the fallback should succeed.
    let proxy = super::FallbackProxy::<RenderStore>::new(primary, vec![secondary.into()], false);
    proxy
        .open_payload(digest)
        .await
        .expect("payload should be loadable via the secondary");
}

#[rstest]
#[tokio::test]
async fn test_try_from_fallback_maybe_to_render_fails_without_render_creation(
    tmpdir: tempfile::TempDir,
) {
    init_logging();

    let primary = MaybeOpenFsRepository::<MaybeRenderStore>::create(tmpdir.path().join("primary"))
        .await
        .unwrap()
        .without_render_creation()
        .opened()
        .await
        .unwrap();
    let proxy = super::FallbackProxy::<MaybeRenderStore>::new(primary, vec![], false);

    let err = super::FallbackProxy::<RenderStore>::try_from(proxy)
        .expect_err("conversion should fail when render creation is disabled");
    assert!(
        matches!(
            err,
            crate::storage::OpenRepositoryError::PathNotInitialized { .. }
        ),
        "conversion should fail with PathNotInitialized when renders are unavailable"
    );
}

#[rstest]
#[tokio::test]
async fn test_try_from_fallback_maybe_to_render_succeeds_after_render_store_exists(
    tmpdir: tempfile::TempDir,
) {
    init_logging();

    let primary = MaybeOpenFsRepository::<MaybeRenderStore>::create(tmpdir.path().join("primary"))
        .await
        .unwrap()
        .opened()
        .await
        .unwrap();
    primary
        .fs_impl
        .try_render_store()
        .expect("create render store before conversion");

    let proxy = super::FallbackProxy::<MaybeRenderStore>::new(primary, vec![], false);
    let converted = super::FallbackProxy::<RenderStore>::try_from(proxy);
    assert!(
        converted.is_ok(),
        "conversion should succeed after render store exists"
    );
}
