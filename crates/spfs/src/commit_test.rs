// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use rstest::rstest;

use super::{commit_layer, commit_platform};
use crate::Error;

use crate::fixtures::*;
#[rstest]
#[tokio::test]
async fn test_commit_empty(tmpdir: tempdir::TempDir) {
    let root = tmpdir.path().to_string_lossy().to_string();
    let repo = crate::storage::RepositoryHandle::from(
        crate::storage::fs::FSRepository::create(root)
            .await
            .unwrap(),
    );
    let storage = crate::runtime::Storage::new(repo);
    let mut rt = storage.create_runtime().await.unwrap();
    if let Err(Error::NothingToCommit) = commit_layer(&mut rt).await {
        // ok
    } else {
        panic!("expected nothing to commit")
    }

    if let Err(Error::NothingToCommit) = commit_platform(&mut rt).await {
        // ok
    } else {
        panic!("expected nothing to commit")
    }
}
