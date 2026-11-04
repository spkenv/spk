// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use rstest::rstest;
use serial_test::serial;

use crate::fixtures::{TempRepo, tmprepo};
use crate::prelude::*;
use crate::{Error, encoding, graph, runtime, storage};

#[rstest]
#[tokio::test]
#[serial(env)]
async fn reports_no_active_runtime(#[future] tmprepo: TempRepo) {
    let repo = tmprepo.await;
    let runtime_env = "SPFS_RUNTIME";
    let saved_runtime = std::env::var_os(runtime_env);

    // Safety: process environment is shared mutable state. This test uses
    // serial(env) so it does not race with other env-mutating tests.
    unsafe {
        std::env::remove_var(runtime_env);
    }

    let diagnostics_err =
        super::find_path_providers_in_spfs_runtime_with_diagnostics("/spfs/does-not-exist", &repo)
            .await
            .expect_err("expected no active runtime error");
    assert!(matches!(diagnostics_err, Error::NoActiveRuntime));

    let legacy_err = super::find_path_providers_in_spfs_runtime("/spfs/does-not-exist", &repo)
        .await
        .expect_err("expected no active runtime error");
    assert!(matches!(legacy_err, Error::NoActiveRuntime));

    // Safety: process environment is shared mutable state. This test uses
    // serial(env) so it does not race with other env-mutating tests.
    unsafe {
        match saved_runtime {
            Some(val) => std::env::set_var(runtime_env, val),
            None => std::env::remove_var(runtime_env),
        }
    }
}

#[tokio::test]
#[serial(env)]
async fn marks_skipped_unknown_for_missing_runtime_stack_object() {
    let config = crate::get_config().expect("get config");
    let fs_repo = config
        .get_opened_local_repository()
        .await
        .expect("open local repository");
    let repo = storage::RepositoryHandle::from(fs_repo.clone());
    let storage = runtime::Storage::new(fs_repo).expect("create runtime storage");
    let mut runtime = storage
        .create_owned_runtime()
        .await
        .expect("create owned runtime");
    runtime.push_digest(encoding::NULL_DIGEST.into());
    runtime
        .save_state_to_storage()
        .await
        .expect("save runtime state");

    let runtime_env = "SPFS_RUNTIME";
    let saved_runtime = std::env::var_os(runtime_env);
    // Safety: process environment is shared mutable state. This test uses
    // serial(env) so it does not race with other env-mutating tests.
    unsafe {
        std::env::set_var(runtime_env, runtime.name());
    }

    let result =
        super::find_path_providers_in_spfs_runtime_with_diagnostics("/spfs/does-not-exist", &repo)
            .await
            .expect("search should complete");
    assert!(result.providers.is_empty());
    assert!(result.skipped_unknown_objects);

    // Safety: process environment is shared mutable state. This test uses
    // serial(env) so it does not race with other env-mutating tests.
    unsafe {
        match saved_runtime {
            Some(val) => std::env::set_var(runtime_env, val),
            None => std::env::remove_var(runtime_env),
        }
    }
}

#[tokio::test]
#[serial(env)]
async fn marks_skipped_unknown_for_missing_layer_manifest_object() {
    let config = crate::get_config().expect("get config");
    let fs_repo = config
        .get_opened_local_repository()
        .await
        .expect("open local repository");
    let repo = storage::RepositoryHandle::from(fs_repo.clone());
    let storage = runtime::Storage::new(fs_repo).expect("create runtime storage");
    let mut runtime = storage
        .create_owned_runtime()
        .await
        .expect("create owned runtime");

    let layer = graph::Layer::new(encoding::NULL_DIGEST.into());
    repo.write_object(&layer)
        .await
        .expect("write layer with missing manifest");
    runtime.push_digest(layer.digest().expect("get layer digest for stack"));
    runtime
        .save_state_to_storage()
        .await
        .expect("save runtime state");

    let runtime_env = "SPFS_RUNTIME";
    let saved_runtime = std::env::var_os(runtime_env);
    // Safety: process environment is shared mutable state. This test uses
    // serial(env) so it does not race with other env-mutating tests.
    unsafe {
        std::env::set_var(runtime_env, runtime.name());
    }

    let result =
        super::find_path_providers_in_spfs_runtime_with_diagnostics("/spfs/does-not-exist", &repo)
            .await
            .expect("search should complete");
    assert!(result.providers.is_empty());
    assert!(result.skipped_unknown_objects);

    // Safety: process environment is shared mutable state. This test uses
    // serial(env) so it does not race with other env-mutating tests.
    unsafe {
        match saved_runtime {
            Some(val) => std::env::set_var(runtime_env, val),
            None => std::env::remove_var(runtime_env),
        }
    }
}

#[tokio::test]
#[serial(env)]
async fn does_not_mark_skipped_unknown_when_runtime_stack_is_empty() {
    let config = crate::get_config().expect("get config");
    let fs_repo = config
        .get_opened_local_repository()
        .await
        .expect("open local repository");
    let repo = storage::RepositoryHandle::from(fs_repo.clone());
    let storage = runtime::Storage::new(fs_repo).expect("create runtime storage");
    let runtime = storage
        .create_owned_runtime()
        .await
        .expect("create owned runtime");
    runtime
        .save_state_to_storage()
        .await
        .expect("save runtime state");

    let runtime_env = "SPFS_RUNTIME";
    let saved_runtime = std::env::var_os(runtime_env);
    // Safety: process environment is shared mutable state. This test uses
    // serial(env) so it does not race with other env-mutating tests.
    unsafe {
        std::env::set_var(runtime_env, runtime.name());
    }

    let result =
        super::find_path_providers_in_spfs_runtime_with_diagnostics("/spfs/does-not-exist", &repo)
            .await
            .expect("search should complete");
    assert!(result.providers.is_empty());
    assert!(!result.skipped_unknown_objects);

    // Safety: process environment is shared mutable state. This test uses
    // serial(env) so it does not race with other env-mutating tests.
    unsafe {
        match saved_runtime {
            Some(val) => std::env::set_var(runtime_env, val),
            None => std::env::remove_var(runtime_env),
        }
    }
}
