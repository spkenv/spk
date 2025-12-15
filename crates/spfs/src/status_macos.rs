// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

//! macOS implementation of runtime status management.
//!
//! On macOS, SPFS uses macFUSE for the virtual filesystem layer.
//! Unlike Linux, macOS does not have overlayfs or mount namespaces,
//! so the runtime model is simplified to FUSE-only operation.

use crate::storage::fs::RenderSummary;
use crate::{Error, Result, bootstrap, env, runtime};

/// Remount the given runtime as configured.
pub async fn remount_runtime(rt: &runtime::Runtime) -> Result<()> {
    let command = bootstrap::build_spfs_remount_command(rt)?;
    let mut cmd = std::process::Command::new(command.executable);
    cmd.args(command.args);
    tracing::debug!("{:?}", cmd);
    let res = tokio::task::spawn_blocking(move || cmd.status())
        .await?
        .map_err(|err| Error::process_spawn_error("spfs-enter --remount", err, None))?;
    if res.code() != Some(0) {
        Err(Error::String(format!(
            "Failed to re-mount runtime filesystem: spfs-enter --remount failed with code {:?}",
            res.code()
        )))
    } else {
        Ok(())
    }
}

/// Exit the given runtime as configured, this should only ever be called with the active runtime
pub async fn exit_runtime(rt: &runtime::Runtime) -> Result<()> {
    let command = bootstrap::build_spfs_exit_command(rt)?;
    let mut cmd = std::process::Command::new(command.executable);
    cmd.args(command.args);
    cmd.stderr(std::process::Stdio::piped());
    tracing::debug!("{:?}", cmd);
    let res = tokio::task::spawn_blocking(move || cmd.output())
        .await?
        .map_err(|err| Error::process_spawn_error("spfs-enter --exit", err, None))?;
    if res.status.code() != Some(0) {
        let out = String::from_utf8_lossy(&res.stderr);
        Err(Error::String(format!(
            "Failed to tear-down runtime filesystem: spfs-enter --exit failed with code {:?}: {}",
            res.status.code(),
            out.trim()
        )))
    } else {
        Ok(())
    }
}

/// Turn the given runtime into a durable runtime, this should only
/// ever be called with the active runtime.
///
/// On macOS with FUSE-only backend, durable runtimes are not supported.
pub async fn make_runtime_durable(_rt: &runtime::Runtime) -> Result<()> {
    Err(Error::RuntimeChangeToDurableError(
        "macFUSE backend does not support durable runtimes".to_string(),
    ))
}

/// Change the current spfs runtime into a durable rt and reinitialize it.
///
/// On macOS with FUSE-only backend, durable runtimes are not supported.
pub async fn change_to_durable_runtime(_rt: &mut runtime::Runtime) -> Result<RenderSummary> {
    Err(Error::RuntimeChangeToDurableError(
        "macFUSE backend does not support durable runtimes".to_string(),
    ))
}

/// Reinitialize the current spfs runtime as rt (in case of runtime config changes).
///
/// # Safety
///
/// This function sets environment variables, see [`std::env::set_var`] for
/// more details on safety.
pub async unsafe fn reinitialize_runtime(rt: &mut runtime::Runtime) -> Result<RenderSummary> {
    // Safety: the responsibility of the caller.
    let configurator = unsafe { env::RuntimeConfigurator.current_runtime(rt)? };

    tracing::debug!("computing runtime manifest");
    let _manifest = super::compute_runtime_manifest(rt).await?;
    configurator.ensure_mounts_already_exist().await?;

    let with_root = configurator.become_root()?;

    // We don't need to unmount anything on macOS since there are no mount namespaces.
    // Just remount the FUSE environment to update the routers state.
    mount_env_for_backend(&with_root, rt).await?;
    with_root.become_original_user()?;
    Ok(RenderSummary::default())
}

async fn mount_env_for_backend(
    with_root: &env::RootConfigurator,
    rt: &runtime::Runtime,
) -> Result<()> {
    match rt.config.mount_backend {
        runtime::MountBackend::FuseOnly | runtime::MountBackend::FuseWithScratch => {
            with_root.mount_env_fuse(rt).await
        }
        _ => Err(Error::String(format!(
            "This binary was not compiled with support for {} on macOS",
            rt.config.mount_backend
        ))),
    }
}

/// Initialize the current runtime as rt.
///
/// This function will run blocking IO on the current thread. On macOS,
/// the runtime model is simpler than Linux as there are no mount namespaces.
pub async fn initialize_runtime(rt: &mut runtime::Runtime) -> Result<RenderSummary> {
    // Before rendering the runtime's layers, prepare any live layers
    rt.prepare_live_layers().await?;

    tracing::debug!("computing runtime manifest");
    let _manifest = super::compute_runtime_manifest(rt).await?;

    // On macOS, we don't have mount namespaces, so we need unique runtime
    // directories per runtime since they're shared across processes.
    // Use the macOS-approved cache directory (~/Library/Caches/spfs/runtimes/)
    let runtime_root = dirs::cache_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
        .join("spfs")
        .join("runtimes")
        .join(rt.name());
    rt.config.upper_dir = runtime_root.join("upper");
    rt.config.lower_dir = runtime_root.join("lower");
    rt.config.work_dir = runtime_root.join("work");
    rt.config.sh_startup_file = runtime_root.join("startup.sh");
    rt.config.csh_startup_file = runtime_root.join(".cshrc");
    rt.config.runtime_dir = Some(runtime_root);

    rt.save_state_to_storage().await?;

    let configurator = env::RuntimeConfigurator;
    let with_root = configurator.become_root()?;
    with_root.ensure_mount_targets_exist(&rt.config)?;

    mount_env_for_backend(&with_root, rt).await?;
    with_root.become_original_user()?;
    Ok(RenderSummary::default())
}
