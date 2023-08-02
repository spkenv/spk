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
        .map_err(|err| Error::process_spawn_error("spfs-enter --remount".to_owned(), err, None))?;
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
    // Not using `tokio::process` here because it relies on `SIGCHLD` to know
    // when the process is done, which can be unreliable if something else
    // is trapping signals, like the tarpaulin code coverage tool.
    let mut cmd = std::process::Command::new(command.executable);
    cmd.args(command.args);
    cmd.stderr(std::process::Stdio::piped());
    tracing::debug!("{:?}", cmd);
    let res = tokio::task::spawn_blocking(move || cmd.output())
        .await?
        .map_err(|err| Error::process_spawn_error("spfs-enter --exit".to_owned(), err, None))?;
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

/// Get the repository that is being used as the backing storage for the given
/// runtime.
///
/// This is usually the local repository but it may be a remote repository
/// depending on the configuration of the spfs filesystem backend.
pub async fn get_runtime_backing_repo(
    rt: &runtime::Runtime,
) -> Result<crate::storage::RepositoryHandle> {
    let config = get_config()?;
    if rt.config.mount_backend.requires_localization() {
        config.get_local_repository_handle().await
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
        Ok(crate::storage::ProxyRepository::from_config(proxy_config)
            .await?
            .into())
    }
}

/// Calculate the file manifest for the layers in the given runtime.
///
/// The returned manifest DOES NOT include any active changes to the runtime.
pub async fn compute_runtime_manifest(rt: &runtime::Runtime) -> Result<tracking::Manifest> {
    let spec = rt.status.stack.iter().cloned().collect();
    super::compute_environment_manifest(&spec, &get_runtime_backing_repo(rt).await?).await
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
///
/// This function will run blocking IO on the current thread. Although this is not ideal,
/// the mount namespacing operated per-thread and so restricts our ability to move execution.
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

    let in_namespace = env::RuntimeConfigurator::default().current_runtime(rt)?;

    tracing::debug!("computing runtime manifest");
    let manifest = compute_runtime_manifest(rt).await?;
    in_namespace.ensure_mounts_already_exist().await?;
    const LAZY: bool = true; // because we are about to re-mount over it
    let with_root = in_namespace.become_root()?;
    with_root.unmount_env(rt, LAZY).await?;
    match rt.config.mount_backend {
        runtime::MountBackend::OverlayFsWithRenders => {
            with_root
                .mount_env_overlayfs(rt, &render_result.paths_rendered)
                .await?;
            with_root.mask_files(&rt.config, manifest).await?;
        }
        #[cfg(feature = "fuse-backend")]
        runtime::MountBackend::OverlayFsWithFuse => {
            // Switch to using a different lower_dir otherwise if we use the
            // same one as the previous runtime when it lazy unmounts it will
            // unmount our active lower_dir.
            rt.rotate_lower_dir().await?;
            rt.save_state_to_storage().await?;

            with_root.mount_fuse_lower_dir(rt).await?;
            with_root
                .mount_env_overlayfs(rt, &render_result.paths_rendered)
                .await?;
        }
        #[cfg(feature = "fuse-backend")]
        runtime::MountBackend::FuseOnly => {
            with_root.mount_env_fuse(rt).await?;
        }
        #[allow(unreachable_patterns)]
        _ => {
            return Err(Error::String(format!(
                "This binary was not compiled with support for {}",
                rt.config.mount_backend
            )))
        }
    }
    with_root.become_original_user()?;
    Ok(render_result.render_summary)
}

/// Initialize the current runtime as rt.
///
/// This function will run blocking IO on the current thread. Although this is not ideal,
/// the mount namespacing operated per-thread and so restricts our ability to move execution.
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

    let in_namespace = env::RuntimeConfigurator::default().enter_mount_namespace()?;
    rt.config.mount_namespace = Some(in_namespace.mount_namespace().to_path_buf());
    rt.save_state_to_storage().await?;

    let with_root = in_namespace.become_root()?;
    with_root.privatize_existing_mounts().await?;
    with_root.ensure_mount_targets_exist(&rt.config)?;
    match rt.config.mount_backend {
        runtime::MountBackend::OverlayFsWithRenders => {
            with_root.mount_runtime(&rt.config)?;
            with_root.setup_runtime(rt).await?;
            with_root
                .mount_env_overlayfs(rt, &render_result.paths_rendered)
                .await?;
            with_root.mask_files(&rt.config, manifest).await?;
        }
        #[cfg(feature = "fuse-backend")]
        runtime::MountBackend::OverlayFsWithFuse => {
            with_root.mount_runtime(&rt.config)?;
            with_root.setup_runtime(rt).await?;
            with_root.mount_fuse_lower_dir(rt).await?;
            with_root
                .mount_env_overlayfs(rt, &render_result.paths_rendered)
                .await?;
        }
        #[cfg(feature = "fuse-backend")]
        runtime::MountBackend::FuseOnly => {
            with_root.mount_env_fuse(rt).await?;
        }
        #[allow(unreachable_patterns)]
        _ => {
            return Err(Error::String(format!(
                "This binary was not compiled with support for {}",
                rt.config.mount_backend
            )))
        }
    }
    with_root.become_original_user()?;
    Ok(render_result.render_summary)
}
