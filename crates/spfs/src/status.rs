// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use super::config::get_config;
use super::resolve::{resolve_overlay_dirs, resolve_stack_to_layers};
use crate::{bootstrap, env, prelude::*, runtime, tracking, Error, Result};

static SPFS_RUNTIME: &str = "SPFS_RUNTIME";

/// Unlock the current runtime file system so that it can be modified.
///
/// Once modified, active changes can be committed
///
/// Errors:
/// - [`spfs::Error::NoActiveRuntime`]: if there is no active runtime
/// - [`spfs::Error::RuntimeAlreadyEditable`]: if the active runtime is already editable
/// - if there are issues remounting the filesystem
pub async fn make_active_runtime_editable() -> Result<()> {
    let mut rt = active_runtime().await?;
    if rt.status.editable {
        return Err(Error::RuntimeAlreadyEditable);
    }

    rt.status.editable = true;
    remount_runtime(&rt).await?;
    rt.save_state_to_storage().await
}

/// Remount the given runtime as configured.
pub async fn remount_runtime(rt: &runtime::Runtime) -> Result<()> {
    let command = bootstrap::build_spfs_remount_command(rt)?;
    let mut cmd = tokio::process::Command::new(command.executable);
    cmd.args(command.args);
    tracing::debug!("{:?}", cmd);
    let res = cmd.status().await?;
    if res.code() != Some(0) {
        Err("Failed to re-mount runtime filesystem".into())
    } else {
        Ok(())
    }
}

/// Calculate the file manifest for the layers in the given runtime.
///
/// The returned manifest DOES NOT include any active changes to the runtime.
pub async fn compute_runtime_manifest(rt: &runtime::Runtime) -> Result<tracking::Manifest> {
    let config = get_config()?;
    let repo = config.get_repository().await?;

    let layers = resolve_stack_to_layers(rt.status.stack.iter(), None).await?;
    let mut manifest = tracking::Manifest::default();
    for layer in layers.iter().rev() {
        manifest.update(&repo.read_manifest(layer.manifest).await?.unlock())
    }
    Ok(manifest)
}

/// Return the currently active runtime
///
/// # Errors:
/// - [`spfs::Error::NoActiveRuntime`] if there is no runtime detected
/// - [`spfs::Error::UnkownRuntime`] if the environment references a
///   runtime that is not in the configured runtime storage
/// - other issues loading the config or accessing the runtime data
pub async fn active_runtime() -> Result<runtime::Runtime> {
    let name = std::env::var(SPFS_RUNTIME).map_err(|_| Error::NoActiveRuntime)?;
    let config = get_config()?;
    let storage = config.get_runtime_storage().await?;
    storage.read_runtime(name).await
}

/// Reinitialize the current spfs runtime as rt (in case of runtime config changes).
pub async fn reinitialize_runtime(rt: &runtime::Runtime) -> Result<()> {
    let dirs = resolve_overlay_dirs(rt).await?;
    tracing::debug!("computing runtime manifest");
    let manifest = compute_runtime_manifest(rt).await?;

    let original = env::become_root()?;
    env::ensure_mounts_already_exist()?;
    env::unmount_env()?;
    env::mount_env(rt, &dirs)?;
    env::mask_files(&rt.config, &manifest, original.uid)?;
    env::become_original_user(original)?;
    env::drop_all_capabilities()?;
    Ok(())
}

/// Initialize the current runtime as rt.
pub async fn initialize_runtime(rt: &runtime::Runtime) -> Result<()> {
    let dirs = resolve_overlay_dirs(rt).await?;
    tracing::debug!("computing runtime manifest");
    let manifest = compute_runtime_manifest(rt).await?;
    env::enter_mount_namespace()?;
    let original = env::become_root()?;
    env::privatize_existing_mounts()?;
    env::ensure_mount_targets_exist(&rt.config)?;
    env::mount_runtime(&rt.config)?;
    env::setup_runtime(rt).await?;
    env::mount_env(rt, &dirs)?;
    env::mask_files(&rt.config, &manifest, original.uid)?;
    env::become_original_user(original)?;
    env::drop_all_capabilities()?;
    Ok(())
}
