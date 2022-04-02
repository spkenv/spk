// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::sync::Arc;

use rstest::fixture;
use spfs::{prelude::*, runtime};
use tokio::sync::{Mutex, MutexGuard};

use crate::storage;

lazy_static::lazy_static! {
    static ref SPFS_RUNTIME_LOCK: Mutex<runtime::Runtime> = Mutex::new(spfs::active_runtime().expect("Tests must be run in an spfs runtime"));
}

pub struct RuntimeLock {
    original_config: spfs::Config,
    pub runtime: MutexGuard<'static, spfs::runtime::Runtime>,
    pub tmprepo: Arc<storage::RepositoryHandle>,
    pub tmpdir: tempdir::TempDir,
}

impl Drop for RuntimeLock {
    fn drop(&mut self) {
        std::env::remove_var("SPFS_STORAGE_RUNTIMES");
        std::env::remove_var("SPFS_STORAGE_ROOT");
        self.original_config
            .clone()
            .make_current()
            .expect("Failed to reset spfs config after test");
    }
}

pub struct TempRepo {
    pub repo: Arc<storage::RepositoryHandle>,
    pub tmpdir: tempdir::TempDir,
}

impl std::ops::Deref for TempRepo {
    type Target = storage::RepositoryHandle;

    fn deref(&self) -> &Self::Target {
        &*self.repo
    }
}

pub fn init_logging() {
    let sub = tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(tracing::Level::TRACE)
        .without_time()
        .with_test_writer()
        .finish();
    let _ = tracing::subscriber::set_global_default(sub);
}

/// Returns an empty spfs layer object for easy testing
pub fn empty_layer() -> spfs::graph::Layer {
    spfs::graph::Layer {
        manifest: Default::default(),
    }
}

/// Returns the digest for an empty spfs layer.
pub fn empty_layer_digest() -> spfs::Digest {
    empty_layer()
        .digest()
        .expect("Empty layer should have valid digest")
}

#[fixture]
pub fn tmpdir() -> tempdir::TempDir {
    tempdir::TempDir::new("spk-test-").expect("Failed to establish temporary directory for testing")
}

#[fixture]
pub fn tmprepo() -> storage::RepositoryHandle {
    storage::RepositoryHandle::Mem(Default::default())
}

/// Establishes a temporary spfs repo on disk.
///
/// This repo comes prefilled with an empty layer and object
/// for use in generating test data to sync around.
#[fixture]
pub async fn spfsrepo() -> TempRepo {
    let tmpdir = tempdir::TempDir::new("spk-test-spfs-repo")
        .expect("failed to establish tmpdir for spfs runtime");
    let storage_root = tmpdir.path().join("repo");
    let spfs_repo = spfs::storage::fs::FSRepository::create(&storage_root)
        .await
        .expect("failed to establish temporary local repo for test");
    let written = spfs_repo
        .write_data(Box::pin(std::io::Cursor::new(b"")))
        .await
        .expect("failed to add an empty object to spfs");
    let empty_manifest = spfs::graph::Manifest::default();
    let empty_layer = empty_layer();
    let _ = spfs_repo
        .write_object(&empty_layer.into())
        .await
        .expect("failed to save empty layer to spfs repo");
    let _ = spfs_repo
        .write_object(&empty_manifest.into())
        .await
        .expect("failed to save empty manifest to spfs repo");
    assert_eq!(written.0, spfs::encoding::EMPTY_DIGEST.into());

    let repo = Arc::new(storage::RepositoryHandle::SPFS(spfs_repo.into()));
    TempRepo { tmpdir, repo }
}

/// Establishes a segregated spfs runtime for use in the test.
///
/// This is a managed resource, and will cause all tests that use
/// it to run serially.
#[fixture]
pub async fn spfs_runtime() -> RuntimeLock {
    init_logging();

    // because these tests are all async, anything that is interacting
    // with spfs must be forced to run one-at-a-time
    let mut runtime = SPFS_RUNTIME_LOCK.lock().await;

    let original_config = spfs::get_config()
        .expect("failed to get original spfs config")
        .as_ref()
        .clone();

    let tmprepo = spfsrepo().await;
    let storage_root = tmprepo.tmpdir.path().join("repo");

    let mut new_config = original_config.clone();
    // preserve the runtime root in case it was inferred in the original one
    let runtimes_root = original_config.storage.runtime_root();
    std::env::set_var("SPFS_STORAGE_RUNTIMES", &runtimes_root);
    new_config.storage.runtimes = Some(runtimes_root);
    // update the config to use our temp dir for local storage
    std::env::set_var("SPFS_STORAGE_ROOT", &storage_root);
    new_config.storage.root = storage_root;

    new_config
        .make_current()
        .expect("failed to update spfs config for test");

    runtime
        .reset_stack()
        .expect("Failed to reset runtime stack");
    runtime
        .reset_all()
        .expect("Failed to reset runtime changes");
    spfs::remount_runtime(&runtime)
        .await
        .expect("failed to reset runtime for test");

    RuntimeLock {
        original_config,
        runtime,
        tmpdir: tmprepo.tmpdir,
        tmprepo: tmprepo.repo,
    }
}

/// A simple trait for use in test writing that allows something to be
/// ensured to exist and be usable, whatever that means in context.
pub trait Ensure {
    fn ensure(&self);
}

impl Ensure for std::path::PathBuf {
    fn ensure(&self) {
        if let Some(parent) = self.parent() {
            std::fs::create_dir_all(parent).expect("failed to ensure parent dir for file");
        }
        std::fs::write(self, b"").expect("failed to ensure empty file");
    }
}
