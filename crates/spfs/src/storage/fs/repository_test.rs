// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use super::{
    DefaultRenderStoreCreationPolicy,
    FsHashStore,
    FsRepositoryOps,
    MaybeOpenFsRepository,
    MaybeRenderStore,
    OpenFsRepositoryImpl,
    RenderStore,
    RenderStoreCreationPolicy,
};
use crate::storage::{
    OpenRepositoryError,
    OpenRepositoryResult,
    RenderStoreForUser,
    TryRenderStore,
};

#[derive(Clone, Debug)]
struct RecordingRenderStore;

fn recorded_user_paths() -> &'static Mutex<Vec<PathBuf>> {
    static RECORDED_USER_PATHS: OnceLock<Mutex<Vec<PathBuf>>> = OnceLock::new();
    RECORDED_USER_PATHS.get_or_init(|| Mutex::new(Vec::new()))
}

impl DefaultRenderStoreCreationPolicy for RecordingRenderStore {
    fn default_creation_policy() -> RenderStoreCreationPolicy {
        RenderStoreCreationPolicy::DoNotCreate
    }
}

impl RenderStoreForUser for RecordingRenderStore {
    type RenderStore = Self;

    fn render_store_for_user(
        _creation_policy: RenderStoreCreationPolicy,
        _url: url::Url,
        _root: &Path,
        username: &Path,
    ) -> OpenRepositoryResult<Self> {
        recorded_user_paths()
            .lock()
            .expect("recorded user paths mutex should not be poisoned")
            .push(username.to_path_buf());
        Ok(Self)
    }
}

impl TryRenderStore for RecordingRenderStore {
    fn try_render_store(&self) -> OpenRepositoryResult<Cow<'_, RenderStore>> {
        Err(OpenRepositoryError::RenderStorageUnavailable)
    }

    fn proxy_path(&self) -> Option<Cow<'_, Path>> {
        None
    }
}

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
async fn test_render_store_for_user_create_if_missing_returns_non_not_found_errors() {
    let tmpdir = tempfile::Builder::new()
        .prefix("spfs-test-")
        .tempdir()
        .unwrap();
    let root = tmpdir.path().join("repo");
    std::fs::create_dir_all(&root).unwrap();

    let username = PathBuf::from("test-user-enotdir");
    let renders_dir = root.join("renders").join(&username);
    std::fs::create_dir_all(renders_dir.parent().unwrap()).unwrap();
    std::fs::write(&renders_dir, b"not a directory").unwrap();

    let url = url::Url::from_directory_path(&root).unwrap();
    let err = RenderStore::render_store_for_user(
        RenderStoreCreationPolicy::CreateIfMissing,
        url,
        &root,
        &username,
    )
    .expect_err("create-if-missing should return non-NotFound metadata errors");

    assert!(
        matches!(
            err,
            crate::storage::OpenRepositoryError::PathNotInitialized { .. }
        ),
        "create-if-missing should preserve metadata errors that are not NotFound"
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

#[tokio::test]
async fn test_renders_for_all_users_passes_username_segment_to_render_store_for_user() {
    let tmpdir = tempfile::Builder::new()
        .prefix("spfs-test-")
        .tempdir()
        .unwrap();
    let root = tmpdir.path().join("repo");
    std::fs::create_dir_all(root.join("objects")).unwrap();
    std::fs::create_dir_all(root.join("payloads")).unwrap();

    let username = "test-user-segment";
    std::fs::create_dir_all(root.join("renders").join(username)).unwrap();

    recorded_user_paths()
        .lock()
        .expect("recorded user paths mutex should not be poisoned")
        .clear();

    let repo = OpenFsRepositoryImpl::<RecordingRenderStore> {
        objects: FsHashStore::open_unchecked(root.join("objects")),
        payloads: FsHashStore::open_unchecked(root.join("payloads")),
        rs_impl: RecordingRenderStore,
        root: root.clone(),
        tag_namespace: None,
    };

    let renders = repo.renders_for_all_users().unwrap();
    assert_eq!(
        renders.len(),
        1,
        "one user render directory should produce one repository entry"
    );
    assert_eq!(
        renders[0].0, username,
        "returned username should match the renders directory name"
    );

    let recorded = recorded_user_paths()
        .lock()
        .expect("recorded user paths mutex should not be poisoned")
        .clone();
    assert_eq!(
        recorded,
        vec![PathBuf::from(username)],
        "render_store_for_user should receive a username segment, not renders/<username>"
    );
    assert!(
        !recorded[0].to_string_lossy().contains("renders"),
        "forwarded path should not include the renders directory prefix"
    );
}

/// A render store that fails with [`OpenRepositoryError::PathNotInitialized`]
/// for usernames containing "bad-".
#[derive(Clone, Debug)]
struct FailingRenderStore;

impl DefaultRenderStoreCreationPolicy for FailingRenderStore {
    fn default_creation_policy() -> RenderStoreCreationPolicy {
        RenderStoreCreationPolicy::DoNotCreate
    }
}

impl RenderStoreForUser for FailingRenderStore {
    type RenderStore = Self;

    fn render_store_for_user(
        _creation_policy: RenderStoreCreationPolicy,
        _url: url::Url,
        _root: &Path,
        username: &Path,
    ) -> OpenRepositoryResult<Self> {
        if username.to_string_lossy().contains("bad-") {
            return Err(OpenRepositoryError::PathNotInitialized {
                path: username.to_path_buf(),
                source: std::io::Error::new(std::io::ErrorKind::NotFound, "simulated"),
            });
        }
        Ok(Self)
    }
}

impl TryRenderStore for FailingRenderStore {
    fn try_render_store(&self) -> OpenRepositoryResult<Cow<'_, RenderStore>> {
        Err(OpenRepositoryError::RenderStorageUnavailable)
    }

    fn proxy_path(&self) -> Option<Cow<'_, Path>> {
        None
    }
}

#[tokio::test]
async fn test_renders_for_all_users_skips_unavailable_per_user_stores() {
    let tmpdir = tempfile::Builder::new()
        .prefix("spfs-test-")
        .tempdir()
        .unwrap();
    let root = tmpdir.path().join("repo");
    std::fs::create_dir_all(root.join("objects")).unwrap();
    std::fs::create_dir_all(root.join("payloads")).unwrap();

    // Create two user directories: one good, one that will fail.
    std::fs::create_dir_all(root.join("renders").join("good-user")).unwrap();
    std::fs::create_dir_all(root.join("renders").join("bad-user")).unwrap();

    let repo = OpenFsRepositoryImpl::<FailingRenderStore> {
        objects: FsHashStore::open_unchecked(root.join("objects")),
        payloads: FsHashStore::open_unchecked(root.join("payloads")),
        rs_impl: FailingRenderStore,
        root: root.clone(),
        tag_namespace: None,
    };

    let renders = repo
        .renders_for_all_users()
        .expect("should not fail even though one user store is unavailable");
    assert_eq!(
        renders.len(),
        1,
        "only the good user should be returned, the bad user should be skipped"
    );
    assert_eq!(
        renders[0].0, "good-user",
        "the returned user should be the one whose render store succeeded"
    );
}
