// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use rstest::rstest;

use super::Committer;
use crate::Error;
use crate::fixtures::*;
use crate::storage::fs::RenderStore;

#[rstest]
#[tokio::test]
async fn test_commit_empty(tmpdir: tempfile::TempDir) {
    let root = tmpdir.path().to_string_lossy().to_string();
    let repo = crate::storage::RepositoryHandle::from(
        crate::storage::fs::MaybeOpenFsRepository::<RenderStore>::create(&root)
            .await
            .unwrap(),
    );
    let storage = crate::runtime::Storage::new(repo).unwrap();
    let repo = crate::storage::RepositoryHandle::from(
        crate::storage::fs::MaybeOpenFsRepository::<RenderStore>::create(root)
            .await
            .unwrap(),
    );
    let mut rt = storage.create_transient_runtime().await.unwrap();
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
