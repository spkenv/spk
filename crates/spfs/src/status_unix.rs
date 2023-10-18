// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use crate::resolve::{resolve_and_render_overlay_dirs, RenderResult};
use crate::storage::fs::RenderSummary;
use crate::{bootstrap, env, runtime, Error, Result};

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
    // Not using `tokio::process` here because it relies on `SIGCHLD` to know
    // when the process is done, which can be unreliable if something else
    // is trapping signals, like the tarpaulin code coverage tool.
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
/// ever be called with the active runtime
pub async fn make_runtime_durable(rt: &runtime::Runtime) -> Result<()> {
    let command = bootstrap::build_spfs_change_to_durable_command(rt)?;
    // Not using `tokio::process` here because it relies on `SIGCHLD` to know
    // when the process is done, which can be unreliable if something else
    // is trapping signals, like the tarpaulin code coverage tool.
    let mut cmd = std::process::Command::new(command.executable);
    cmd.args(command.args);
    cmd.stderr(std::process::Stdio::piped());
    tracing::debug!("Running: {:?}", cmd);
    let res = tokio::task::spawn_blocking(move || cmd.output())
        .await?
        .map_err(|err| Error::process_spawn_error("spfs-enter --make-durable", err, None))?;
    if res.status.code() != Some(0) {
        let out = String::from_utf8_lossy(&res.stderr);
        let exit_code = match res.status.code() {
            Some(n) => n.to_string(),
            None => String::from("unknown"),
        };
        Err(Error::String(format!(
            "Failed to make runtime durable: spfs-enter --make-durable failed with code {:?}: {}",
            exit_code,
            out.trim()
        )))
    } else {
        Ok(())
    }
}

/// Change the current spfs runtime into a durable rt and reinitialize it
/// (after runtime config changes, and syncing anys edits over).
pub async fn change_to_durable_runtime(rt: &mut runtime::Runtime) -> Result<RenderSummary> {
    let in_namespace = env::RuntimeConfigurator::default().current_runtime(rt)?;
    let with_root = in_namespace.become_root()?;

    with_root.change_runtime_to_durable(rt).await?;
    tracing::info!("runtime changed to durable");

    // unmount overlayfs only so it can be remounted below, but not
    // the fuse part (if any) because it isn't changing and trying to
    // unmount and remount fuse in this case hangs.
    const LAZY: bool = true; // because we are about to re-mount over it
    with_root.unmount_env_overlayfs(rt, LAZY).await?;
    tracing::debug!("runtime overlayfs unmounted");

    // remount the overlayfs only, using its new durable path settings
    let render_result = match rt.config.mount_backend {
        runtime::MountBackend::OverlayFsWithRenders => {
            resolve_and_render_overlay_dirs(rt, false).await?
        }
        runtime::MountBackend::OverlayFsWithFuse
        | runtime::MountBackend::FuseOnly
        | runtime::MountBackend::WinFsp => {
            // fuse uses the lowerdir that's defined in the runtime
            // config, which is implicitly added to all overlay mounts
            Default::default()
        }
    };
    with_root
        .mount_env_overlayfs(rt, &render_result.paths_rendered)
        .await?;
    tracing::debug!("runtime overlayfs remounted");

    with_root.become_original_user()?;
    Ok(render_result.render_summary)
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
        runtime::MountBackend::OverlayFsWithFuse
        | runtime::MountBackend::FuseOnly
        | runtime::MountBackend::WinFsp => {
            // fuse uses the lowerdir that's defined in the runtime
            // config, which is implicitly added to all overlay mounts
            Default::default()
        }
    };

    let in_namespace = env::RuntimeConfigurator::default().current_runtime(rt)?;

    tracing::debug!("computing runtime manifest");
    let manifest = super::compute_runtime_manifest(rt).await?;
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
        runtime::MountBackend::OverlayFsWithFuse
        | runtime::MountBackend::FuseOnly
        | runtime::MountBackend::WinFsp => {
            // fuse uses the lowerdir that's defined in the runtime
            // config, which is implicitly added to all overlay mounts
            RenderResult::default()
        }
    };
    tracing::debug!("computing runtime manifest");
    let manifest = super::compute_runtime_manifest(rt).await?;

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
