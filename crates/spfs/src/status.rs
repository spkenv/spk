// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use super::config::get_config;
use super::resolve::{resolve_and_render_overlay_dirs, RenderResult};
use crate::storage::fs::RenderSummary;
use crate::storage::FromConfig;
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
        .map_err(|err| Error::process_spawn_error("spfs-remount".to_owned(), err, None))?;
    if res.code() != Some(0) {
        Err(Error::String(format!(
            "Failed to re-mount runtime filesystem: spfs-remount failed with code {:?}",
            res.code()
        )))
    } else {
        Ok(())
    }
}

/// Calculate the file manifest for the layers in the given runtime.
///
/// The returned manifest DOES NOT include any active changes to the runtime.
pub async fn compute_runtime_manifest(rt: &runtime::Runtime) -> Result<tracking::Manifest> {
    let config = get_config()?;
    let repo = if rt.config.mount_backend.requires_localization() {
        config.get_local_repository_handle().await?
    } else {
        let proxy_config = crate::storage::proxy::Config {
            primary: config.storage.root.to_string_lossy().to_string(),
            secondary: rt
                .config
                .secondary_repositories
                .iter()
                .map(ToString::to_string)
                .collect(),
        };
        crate::storage::ProxyRepository::from_config(proxy_config)
            .await?
            .into()
    };
    let spec = rt.status.stack.iter().cloned().collect();
    super::compute_environment_manifest(&spec, &repo).await
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
pub async fn reinitialize_runtime(rt: &mut runtime::Runtime) -> Result<RenderSummary> {
    let render_result = match rt.config.mount_backend {
        runtime::MountBackend::OverlayFsWithRenders => {
            resolve_and_render_overlay_dirs(rt, false).await?
        }
        runtime::MountBackend::OverlayFsWithFuse | runtime::MountBackend::FuseOnly => {
            // fuse uses the lowerdir that's defined in the runtime
            // config, which is implicitly added to all overlay mounts
            Default::default()
        }
    };
    tracing::debug!("computing runtime manifest");
    let manifest = compute_runtime_manifest(rt).await?;
    env::ensure_mounts_already_exist().await?;
    const LAZY: bool = true; // because we are about to re-mount over it
    env::unmount_env_fuse(rt, LAZY).await?;
    let original = env::become_root()?;
    env::unmount_env(rt, LAZY).await?;
    match rt.config.mount_backend {
        runtime::MountBackend::OverlayFsWithRenders => {
            env::mount_env_overlayfs(rt, &render_result.paths_rendered).await?;
            env::mask_files(&rt.config, &manifest, original.uid).await?;
        }
        #[cfg(feature = "fuse-backend")]
        runtime::MountBackend::OverlayFsWithFuse => {
            env::mount_fuse_lower_dir(rt, &original).await?;
            env::mount_env_overlayfs(rt, &render_result.paths_rendered).await?;
        }
        #[cfg(feature = "fuse-backend")]
        runtime::MountBackend::FuseOnly => {
            env::mount_env_fuse(rt, &original).await?;
        }
        #[allow(unreachable_patterns)]
        _ => {
            return Err(Error::String(format!(
                "This binary was not compiled with support for {}",
                rt.config.mount_backend
            )))
        }
    }
    env::become_original_user(original)?;
    env::drop_all_capabilities()?;
    Ok(render_result.render_summary)
}

/// Initialize the current runtime as rt.
pub async fn initialize_runtime(rt: &mut runtime::Runtime) -> Result<RenderSummary> {
    let render_result = match rt.config.mount_backend {
        runtime::MountBackend::OverlayFsWithRenders => {
            resolve_and_render_overlay_dirs(
                rt,
                // skip saving the runtime in this step because we will save it after
                // learning the mount namespace below
                true,
            )
            .await?
        }
        runtime::MountBackend::OverlayFsWithFuse | runtime::MountBackend::FuseOnly => {
            // fuse uses the lowerdir that's defined in the runtime
            // config, which is implicitly added to all overlay mounts
            RenderResult::default()
        }
    };
    tracing::debug!("computing runtime manifest");
    let manifest = compute_runtime_manifest(rt).await?;
    env::enter_mount_namespace()?;
    rt.config.mount_namespace =
        env::identify_mount_namespace_of_process(std::process::id()).await?;
    rt.save_state_to_storage().await?;

    let original = env::become_root()?;
    env::privatize_existing_mounts().await?;
    env::ensure_mount_targets_exist(&rt.config)?;
    match rt.config.mount_backend {
        runtime::MountBackend::OverlayFsWithRenders => {
            env::mount_runtime(&rt.config)?;
            env::setup_runtime(rt).await?;
            env::mount_env_overlayfs(rt, &render_result.paths_rendered).await?;
            env::mask_files(&rt.config, &manifest, original.uid).await?;
        }
        #[cfg(feature = "fuse-backend")]
        runtime::MountBackend::OverlayFsWithFuse => {
            env::mount_runtime(&rt.config)?;
            env::setup_runtime(rt).await?;
            env::mount_fuse_lower_dir(rt, &original).await?;
            env::mount_env_overlayfs(rt, &render_result.paths_rendered).await?;
        }
        #[cfg(feature = "fuse-backend")]
        runtime::MountBackend::FuseOnly => {
            env::mount_env_fuse(rt, &original).await?;
        }
        #[allow(unreachable_patterns)]
        _ => {
            return Err(Error::String(format!(
                "This binary was not compiled with support for {}",
                rt.config.mount_backend
            )))
        }
    }
    env::become_original_user(original)?;
    env::drop_all_capabilities()?;
    Ok(render_result.render_summary)
}
