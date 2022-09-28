// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use super::config::get_config;
use super::resolve::{resolve_and_render_overlay_dirs, resolve_stack_to_layers};
use crate::prelude::*;
use crate::{bootstrap, env, runtime, tracking, Error, Result};

static SPFS_RUNTIME: &str = "SPFS_RUNTIME";

/// Unlock the current runtime file system so that it can be modified.
///
/// Once modified, active changes can be committed
///
/// Errors:
/// - [`Error::NoActiveRuntime`]: if there is no active runtime
/// - [`Error::RuntimeAlreadyEditable`]: if the active runtime is already editable
/// - if there are issues remounting the filesystem
pub async fn make_active_runtime_editable() -> Result<()> {
    let mut rt = active_runtime().await?;
    if rt.status.editable {
        return Err(Error::RuntimeAlreadyEditable);
    }

    rt.status.editable = true;
    rt.save_state_to_storage().await?;
    remount_runtime(&rt).await
}

/// Remount the given runtime as configured.
pub async fn remount_runtime(rt: &runtime::Runtime) -> Result<()> {
    let command = bootstrap::build_spfs_remount_command(rt)?;
    // Not using `tokio::process` here because it relies on `SIGCHLD` to know
    // when the process is done, which can be unreliable if something else
    // is trapping signals, like the tarpaulin code coverage tool.
    let mut cmd = std::process::Command::new(command.executable);
    cmd.args(command.args);
    tracing::debug!("{:?}", cmd);
    let res = tokio::task::spawn_blocking(move || cmd.status())
        .await?
        .map_err(|err| Error::process_spawn_error("remount".to_owned(), err, None))?;
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
    let (repo, layers) = tokio::try_join!(
        config.get_local_repository(),
        resolve_stack_to_layers(rt.status.stack.iter(), None)
    )?;
    let mut manifest = tracking::Manifest::default();
    for layer in layers.iter().rev() {
        manifest.update(&repo.read_manifest(layer.manifest).await?.unlock())
    }
    Ok(manifest)
}

/// Return the currently active runtime
///
/// # Errors:
/// - [`Error::NoActiveRuntime`] if there is no runtime detected
/// - [`Error::UnknownRuntime`] if the environment references a
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
    tracing::debug!("computing runtime manifest");
    let (dirs, manifest) = tokio::try_join!(
        resolve_and_render_overlay_dirs(rt),
        compute_runtime_manifest(rt)
    )?;

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
    tracing::debug!("computing runtime manifest");
    let (dirs, manifest) = tokio::try_join!(
        resolve_and_render_overlay_dirs(rt),
        compute_runtime_manifest(rt)
    )?;
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
