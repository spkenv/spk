// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use super::config::{load_config, Config};
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
    let mut rt = active_runtime()?;
    if rt.is_editable() {
        return Err(Error::RuntimeAlreadyEditable);
    }

    rt.set_editable(true)?;
    match remount_runtime(&rt).await {
        Err(err) => {
            rt.set_editable(false)?;
            Err(err)
        }
        Ok(_) => Ok(()),
    }
}

/// Remount the given runtime as configured.
pub async fn remount_runtime(rt: &runtime::Runtime) -> Result<()> {
    let (cmd, args) = bootstrap::build_spfs_remount_command(rt)?;
    let mut cmd = tokio::process::Command::new(cmd);
    cmd.args(&args);
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
    let config = load_config()?;
    let repo = config.get_repository().await?;

    let stack = rt.get_stack();
    let layers = resolve_stack_to_layers(stack.iter(), None).await?;
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
pub fn active_runtime() -> Result<runtime::Runtime> {
    let name = std::env::var(SPFS_RUNTIME).map_err(|_| Error::NoActiveRuntime)?;
    let config = load_config()?;
    let storage = config.get_runtime_storage()?;
    storage.read_runtime(name)
}

/// Reinitialize the current spfs runtime as rt (in case of runtime config changes).
pub async fn reinitialize_runtime(rt: &runtime::Runtime) -> Result<()> {
    let dirs = resolve_overlay_dirs(rt).await?;
    tracing::debug!("computing runtime manifest");
    let manifest = compute_runtime_manifest(rt).await?;

    let original = env::become_root()?;
    env::ensure_mounts_already_exist()?;
    env::unmount_env()?;
    env::mount_env(rt.is_editable(), &dirs)?;
    env::mask_files(&manifest, original.uid)?;
    env::become_original_user(original)?;
    env::drop_all_capabilities()?;
    Ok(())
}

/// Initialize the current runtime as rt.
pub async fn initialize_runtime(rt: &runtime::Runtime, config: &Config) -> Result<()> {
    let dirs = resolve_overlay_dirs(rt).await?;
    tracing::debug!("computing runtime manifest");
    let manifest = compute_runtime_manifest(rt).await?;

    let tmpfs_opts = config
        .filesystem
        .tmpfs_size
        .as_ref()
        .map(|size| format!("size={size}"));

    env::enter_mount_namespace()?;
    let original = env::become_root()?;
    env::privatize_existing_mounts()?;
    env::ensure_mount_targets_exist()?;
    env::mount_runtime(tmpfs_opts.as_deref())?;
    env::setup_runtime()?;
    env::mount_env(rt.is_editable(), &dirs)?;
    env::mask_files(&manifest, original.uid)?;
    env::become_original_user(original)?;
    env::drop_all_capabilities()?;
    Ok(())
}
