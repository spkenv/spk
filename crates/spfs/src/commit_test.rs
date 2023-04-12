// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::sync::Arc;

use rstest::rstest;

use super::Committer;
use crate::fixtures::*;
use crate::Error;

#[rstest]
#[tokio::test]
async fn test_commit_empty(tmpdir: tempfile::TempDir) {
    let root = tmpdir.path().to_string_lossy().to_string();
    let repo = Arc::new(crate::storage::RepositoryHandle::from(
        crate::storage::fs::FSRepository::create(root)
            .await
            .unwrap(),
    ));
    let storage = crate::runtime::Storage::new(repo.clone());
    let mut rt = storage.create_runtime().await.unwrap();
    rt.ensure_required_directories().await.unwrap();
    let committer = Committer::new(&repo);
    match committer.commit_layer(&mut rt).await {
        Err(Error::NothingToCommit) => {}
        res => panic!("expected nothing to commit, got {res:?}"),
    }

    match committer.commit_platform(&mut rt).await {
        Err(Error::NothingToCommit) => {}
        res => panic!("expected nothing to commit, got {res:?}"),
    }
}
