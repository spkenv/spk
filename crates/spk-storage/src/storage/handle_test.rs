// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::sync::Arc;

use rstest::rstest;

use super::RepositoryHandle;
use crate::fixtures::*;
use crate::{IndexedRepository, Repository};

/// Wrap the given handle in an index, as the repository flags do when index
/// use is enabled for a repository.
async fn indexed(repo: Arc<RepositoryHandle>) -> RepositoryHandle {
    RepositoryHandle::Indexed(
        IndexedRepository::generate_from_repo(repo)
            .await
            .expect("failed to generate an index for the test repository"),
    )
}

#[rstest]
#[tokio::test]
async fn test_try_as_spfs_returns_spfs_repo() {
    let repo = make_repo(RepoKind::Spfs).await;

    let spfs_repo = repo
        .try_as_spfs()
        .expect("an spfs repository should expose its spfs storage");

    assert_eq!(spfs_repo.address(), repo.address());
}

#[rstest]
#[tokio::test]
async fn test_try_as_spfs_unwraps_indexed_repo() {
    // An indexed repository wraps another repository, and callers that need
    // spfs-level access must still be able to reach it. Otherwise operations
    // like rendering and exporting silently change behavior depending on
    // whether indexes happen to be enabled.
    let repo = make_repo(RepoKind::Spfs).await;
    let expected_address = repo.address().clone();

    let indexed_handle = indexed(Arc::clone(&repo.repo)).await;

    let spfs_repo = indexed_handle
        .try_as_spfs()
        .expect("an indexed spfs repository should expose the spfs storage it wraps");

    assert_eq!(*spfs_repo.address(), expected_address);
}

#[rstest]
#[case::mem(RepoKind::Mem)]
#[case::indexed_mem(RepoKind::IndexedMem)]
#[tokio::test]
async fn test_try_as_spfs_is_none_without_spfs_storage(#[case] kind: RepoKind) {
    let repo = make_repo(kind).await;

    assert!(
        repo.try_as_spfs().is_none(),
        "{kind:?} has no spfs storage behind it"
    );
}

#[rstest]
fn test_try_as_spfs_is_none_for_runtime_repo() {
    assert!(
        RepositoryHandle::new_runtime().try_as_spfs().is_none(),
        "a runtime repository has no spfs storage behind it"
    );
}

#[rstest]
#[tokio::test]
async fn test_underlying_repo_ref_borrows_wrapped_handle() {
    let repo = make_repo(RepoKind::Mem).await;
    let expected_address = repo.address().clone();

    let RepositoryHandle::Indexed(indexed_repo) = indexed(Arc::clone(&repo.repo)).await else {
        panic!("expected an indexed repository handle");
    };

    assert_eq!(
        *indexed_repo.underlying_repo_ref().address(),
        expected_address
    );
}
