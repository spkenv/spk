// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::path::PathBuf;

use super::{MaybeOpenFsRepository, MaybeRenderStore, RenderStore, RenderStoreCreationPolicy};
use crate::storage::{RenderStoreForUser, TryRenderStore};

#[tokio::test]
async fn test_render_store_for_user_create_if_missing_creates_proxy_dir() {
    let tmpdir = tempfile::Builder::new()
        .prefix("spfs-test-")
        .tempdir()
        .unwrap();
    let root = tmpdir.path().join("repo");
    std::fs::create_dir_all(&root).unwrap();

    let username = PathBuf::from("test-user-create");
    let url = url::Url::from_directory_path(&root).unwrap();

    let store = RenderStore::render_store_for_user(
        RenderStoreCreationPolicy::CreateIfMissing,
        url,
        &root,
        &username,
    )
    .unwrap();

    assert!(
        store.proxy.root().is_dir(),
        "proxy dir should exist when create-if-missing is used"
    );
    assert!(
        store.renders.root().is_dir(),
        "renders dir should exist when create-if-missing is used"
    );
}

#[tokio::test]
async fn test_render_store_for_user_do_not_create_returns_error() {
    let tmpdir = tempfile::Builder::new()
        .prefix("spfs-test-")
        .tempdir()
        .unwrap();
    let root = tmpdir.path().join("repo");
    std::fs::create_dir_all(&root).unwrap();

    let username = PathBuf::from("test-user-no-create");
    let url = url::Url::from_directory_path(&root).unwrap();

    let err = RenderStore::render_store_for_user(
        RenderStoreCreationPolicy::DoNotCreate,
        url,
        &root,
        &username,
    )
    .expect_err("do-not-create should fail when proxy dir does not exist");

    assert!(
        matches!(
            err,
            crate::storage::OpenRepositoryError::PathNotInitialized { .. }
        ),
        "do-not-create should fail with PathNotInitialized"
    );

    let proxy_dir = root.join("renders").join(&username).join("proxy");
    assert!(
        !proxy_dir.exists(),
        "do-not-create should not create the proxy path"
    );
}

#[tokio::test]
async fn test_without_render_creation_disables_lazy_render_store_creation() {
    let tmpdir = tempfile::Builder::new()
        .prefix("spfs-test-")
        .tempdir()
        .unwrap();
    let root = tmpdir.path().join("repo");

    let repo = MaybeOpenFsRepository::<MaybeRenderStore>::create(&root)
        .await
        .unwrap();
    let repo = repo.without_render_creation();
    let opened = repo.opened().await.unwrap();

    let err = opened
        .fs_impl
        .try_render_store()
        .expect_err("without_render_creation should not create missing render store");
    assert!(
        matches!(
            err,
            crate::storage::OpenRepositoryError::PathNotInitialized { .. }
        ),
        "missing render store should still be reported as not initialized"
    );
}

#[tokio::test]
async fn test_try_from_maybe_open_repo_to_render_repo_fails_with_creation_disabled() {
    let tmpdir = tempfile::Builder::new()
        .prefix("spfs-test-")
        .tempdir()
        .unwrap();
    let root = tmpdir.path().join("repo");

    let repo = MaybeOpenFsRepository::<MaybeRenderStore>::create(&root)
        .await
        .unwrap()
        .without_render_creation();
    let err = <MaybeOpenFsRepository<RenderStore>>::try_from(repo)
        .expect_err("conversion should fail when renders have not been created");
    assert!(
        matches!(
            err,
            crate::storage::OpenRepositoryError::PathNotInitialized { .. }
        ),
        "missing render store should fail with PathNotInitialized"
    );
}

#[tokio::test]
async fn test_try_from_maybe_open_repo_to_render_repo_succeeds_after_render_store_exists() {
    let tmpdir = tempfile::Builder::new()
        .prefix("spfs-test-")
        .tempdir()
        .unwrap();
    let root = tmpdir.path().join("repo");

    let repo = MaybeOpenFsRepository::<MaybeRenderStore>::create(&root)
        .await
        .unwrap();
    let opened = repo.opened().await.unwrap();
    opened
        .fs_impl
        .try_render_store()
        .expect("create the render store before conversion");

    let converted = <MaybeOpenFsRepository<RenderStore>>::try_from(repo);
    assert!(
        converted.is_ok(),
        "conversion should succeed once the render store exists"
    );
}
