// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::sync::Arc;

use rstest::rstest;

use crate::fixtures::*;
use crate::prelude::*;

#[rstest]
#[tokio::test]
async fn test_proxy_payload_repair(tmpdir: tempfile::TempDir) {
    init_logging();

    let primary = Arc::new(
        crate::storage::fs::OpenFsRepository::create(tmpdir.path().join("primary"))
            .await
            .unwrap(),
    );
    let secondary = Arc::new(
        crate::storage::fs::OpenFsRepository::create(tmpdir.path().join("secondary"))
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
    let proxy = super::FallbackProxy::new(primary, vec![secondary.into()]);
    proxy
        .open_payload(digest)
        .await
        .expect("payload should be loadable via the secondary");
}
