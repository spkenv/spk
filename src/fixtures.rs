// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::sync::{Mutex, MutexGuard};

use rstest::fixture;
use spfs::runtime;

use crate::storage;

lazy_static::lazy_static! {
    static ref SPFS_RUNTIME_LOCK: Mutex<runtime::Runtime> = Mutex::new(spfs::active_runtime().expect("Tests must be run in an spfs runtime"));
}

pub struct RuntimeLock {
    original_config: spfs::Config,
    pub runtime: MutexGuard<'static, spfs::runtime::Runtime>,
    pub tmprepo: std::sync::Arc<storage::RepositoryHandle>,
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

pub fn init_logging() {
    let sub = tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(tracing::Level::TRACE)
        .without_time()
        .with_test_writer()
        .finish();
    let _ = tracing::subscriber::set_global_default(sub);
}

#[fixture]
pub fn tmpdir() -> tempdir::TempDir {
    tempdir::TempDir::new("spk-test-").expect("Failed to establish temporary directory for testing")
}

#[fixture]
pub fn tmprepo() -> storage::RepositoryHandle {
    storage::RepositoryHandle::Mem(Default::default())
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

    // we also establish a temporary repository in this context for any
    // changes that the test wants to make (commit, render, run, etc)
    let tmpdir = tempdir::TempDir::new("spk-test-spfs-repo")
        .expect("failed to establish tmpdir for spfs runtime");
    let storage_root = tmpdir.path().join("repo");
    let tmprepo = std::sync::Arc::new(storage::RepositoryHandle::SPFS(
        spfs::storage::fs::FSRepository::create(&storage_root)
            .await
            .expect("failed to establish temporary local repo for test")
            .into(),
    ));

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
        tmpdir,
        tmprepo,
    }
}
