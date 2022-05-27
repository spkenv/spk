// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::sync::Arc;

use rstest::rstest;

use super::{commit_layer, commit_platform};
use crate::Error;

use crate::fixtures::*;
#[rstest]
#[tokio::test]
async fn test_commit_empty(tmpdir: tempdir::TempDir) {
    let root = tmpdir.path().to_string_lossy().to_string();
    let repo = Arc::new(crate::storage::RepositoryHandle::from(
        crate::storage::fs::FSRepository::create(root)
            .await
            .unwrap(),
    ));
    let storage = crate::runtime::Storage::new(repo.clone());
    let mut rt = storage.create_runtime().await.unwrap();
    rt.ensure_required_directories().await.unwrap();
    match commit_layer(&mut rt, Arc::clone(&repo)).await {
        Err(Error::NothingToCommit) => {}
        res => panic!("expected nothing to commit, got {res:?}"),
    }

    match commit_platform(&mut rt, Arc::clone(&repo)).await {
        Err(Error::NothingToCommit) => {}
        res => panic!("expected nothing to commit, got {res:?}"),
    }
}
