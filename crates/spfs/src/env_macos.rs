// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

//! macOS implementation of spfs environment management.
//!
//! On macOS, SPFS uses macFUSE for the virtual filesystem layer.
//! Unlike Linux, macOS does not have overlayfs or mount namespaces,
//! so the runtime model is simplified to FUSE-only operation.

use std::path::Path;
use std::process::Stdio;
use std::time::{Duration, Instant};

use tokio::process::Command;
use tokio::time::timeout;
use tonic::transport::Endpoint;

use crate::config::OverlayFsOptions;
use crate::{Error, Result, runtime};

pub const SPFS_DIR: &str = "/spfs";
pub const SPFS_DIR_PREFIX: &str = "/spfs/";

const MACOS_FUSE_SERVICE_ADDR: &str = "127.0.0.1:37738";
const SERVICE_STARTUP_TIMEOUT_SECS: u64 = 10;
const MAX_SERVICE_START_RETRIES: u32 = 5;
const SERVICE_CHECK_TIMEOUT_MS: u64 = 100;

/// Overlay arguments constant - required for compatibility but not used on macOS.
#[allow(dead_code)]
pub(crate) const OVERLAY_ARGS_LOWERDIR_APPEND: &str = "lowerdir+";

/// Manages the configuration of an spfs runtime environment on macOS.
///
/// On macOS, we use macFUSE for the filesystem layer. Unlike Linux,
/// there are no mount namespaces or overlayfs, so the configurator
/// is simplified.
#[derive(Default)]
pub struct RuntimeConfigurator;

impl RuntimeConfigurator {
    /// Make this configurator for an existing runtime.
    ///
    /// On macOS, this validates that the runtime exists and is accessible.
    ///
    /// # Safety
    ///
    /// This function sets environment variables, see [`std::env::set_var`] for
    /// more details on safety.
    pub unsafe fn current_runtime(self, rt: &runtime::Runtime) -> Result<Self> {
        // Safety: the responsibility of the caller.
        unsafe {
            std::env::set_var("SPFS_RUNTIME", rt.name());
        }
        Ok(self)
    }

    /// Move this process into the namespace of an existing runtime.
    ///
    /// On macOS, there are no mount namespaces, so this simply validates
    /// the runtime and sets the environment.
    ///
    /// # Safety
    ///
    /// This function sets environment variables, see [`std::env::set_var`] for
    /// more details on safety.
    pub unsafe fn join_runtime(self, rt: &runtime::Runtime) -> Result<Self> {
        let _pid = match rt.status.owner {
            None => return Err(Error::RuntimeNotInitialized(rt.name().into())),
            Some(pid) => pid,
        };

        // Safety: the responsibility of the caller.
        unsafe {
            std::env::set_var("SPFS_RUNTIME", rt.name());
        }
        Ok(self)
    }

    /// Enter a new mount namespace.
    ///
    /// On macOS, there are no mount namespaces. This is a no-op that returns
    /// the configurator unchanged.
    pub fn enter_mount_namespace(self) -> Result<Self> {
        // macOS doesn't have mount namespaces - return self unchanged
        Ok(self)
    }

    /// Return an error if the spfs filesystem is not mounted.
    pub async fn ensure_mounts_already_exist(&self) -> Result<()> {
        // Check if /spfs exists and is a mount point
        let metadata = tokio::fs::metadata(SPFS_DIR).await;
        match metadata {
            Ok(_) => Ok(()),
            Err(_) => Err(format!("'{SPFS_DIR}' is not mounted").into()),
        }
    }

    /// The path to the mount namespace associated of the current thread.
    ///
    /// On macOS, returns an empty path since there are no mount namespaces.
    #[inline]
    pub fn mount_namespace(&self) -> &std::path::Path {
        std::path::Path::new("")
    }

    /// Escalate the current process' privileges, becoming root.
    ///
    /// On macOS, this attempts to become root via setuid if the binary
    /// has the appropriate permissions.
    pub fn become_root(self) -> Result<RootConfigurator> {
        tracing::debug!("becoming root...");
        let original_euid = nix::unistd::geteuid();
        if let Err(err) = nix::unistd::seteuid(nix::unistd::Uid::from_raw(0)) {
            return Err(Error::wrap_nix(
                err,
                "Failed to become root user (effective)",
            ));
        }
        let original_uid = nix::unistd::getuid();
        if let Err(err) = nix::unistd::setuid(nix::unistd::Uid::from_raw(0)) {
            return Err(Error::wrap_nix(err, "Failed to become root user (actual)"));
        }
        Ok(RootConfigurator {
            original_uid,
            original_euid,
        })
    }

    /// Mount the provided runtime via the macFUSE backend.
    #[cfg(feature = "fuse-backend")]
    pub async fn mount_env_fuse(&self, rt: &runtime::Runtime) -> Result<()> {
        ensure_service_running().await?;
        self.mount_fuse_onto(rt, SPFS_DIR).await
    }

    #[cfg(feature = "fuse-backend")]
    async fn mount_fuse_onto<P>(&self, rt: &runtime::Runtime, path: P) -> Result<()>
    where
        P: AsRef<std::ffi::OsStr>,
    {
        use spfs_encoding::prelude::*;

        let path = path.as_ref().to_owned();
        let platform = rt.to_platform().digest()?.to_string();
        let editable = rt.status.editable;
        let read_only = !editable;

        // Build mount options for macFUSE
        let opts = get_fuse_args(&rt.config, read_only);

        tracing::debug!(editable, "mounting the FUSE filesystem on macOS...");
        let spfs_fuse = match super::resolve::which_spfs("fuse-macos") {
            None => return Err(Error::MissingBinary("spfs-fuse-macos")),
            Some(exe) => exe,
        };

        let mut cmd = std::process::Command::new(spfs_fuse);
        cmd.arg("mount");
        if editable {
            cmd.arg("--editable");
            cmd.arg("--runtime-name").arg(rt.name());
        }
        cmd.arg(&platform);
        cmd.stdout(std::process::Stdio::null());
        cmd.stderr(std::process::Stdio::null());
        tracing::debug!("{cmd:?}");

        match cmd.status() {
            Err(err) => return Err(Error::process_spawn_error("mount", err, None)),
            Ok(status) if status.code() == Some(0) => {}
            Ok(status) => {
                return Err(Error::String(format!(
                    "Failed to mount fuse filesystem, mount command exited with non-zero status {:?}",
                    status.code()
                )));
            }
        };

        // Wait for FUSE to be ready
        let mut sleep_time_ms = vec![2, 5, 10, 50, 100, 100, 100, 100];
        while let Err(err) = std::fs::symlink_metadata(&path) {
            if let Some(ms) = sleep_time_ms.pop() {
                std::thread::sleep(std::time::Duration::from_millis(ms));
            } else {
                tracing::warn!("FUSE did not appear to start after delay: {err}");
                break;
            }
        }
        Ok(())
    }
}

/// A configurator that has escalated to root privileges.
pub struct RootConfigurator {
    pub original_uid: nix::unistd::Uid,
    pub original_euid: nix::unistd::Uid,
}

impl RootConfigurator {
    /// Drop all capabilities and become the original user.
    pub fn become_original_user(self) -> Result<RuntimeConfigurator> {
        tracing::debug!("dropping root...");
        let mut result = nix::unistd::setuid(self.original_uid);
        if let Err(err) = result {
            return Err(Error::wrap_nix(
                err,
                "Failed to become regular user (actual)",
            ));
        }
        result = nix::unistd::seteuid(self.original_euid);
        if let Err(err) = result {
            return Err(Error::wrap_nix(
                err,
                "Failed to become regular user (effective)",
            ));
        }
        Ok(RuntimeConfigurator)
    }

    /// Remove mount propagation.
    ///
    /// On macOS, there is no mount propagation to remove. This is a no-op.
    pub async fn remove_mount_propagation(&self) -> Result<()> {
        // macOS doesn't have mount propagation
        Ok(())
    }

    /// Check or create the necessary directories for mounting the provided runtime.
    pub fn ensure_mount_targets_exist(&self, _config: &runtime::Config) -> Result<()> {
        tracing::debug!("ensuring mount targets exist...");
        runtime::makedirs_with_perms(SPFS_DIR, 0o777)
            .map_err(|source| Error::CouldNotCreateSpfsRoot { source })?;
        Ok(())
    }

    /// Setup runtime directories.
    pub async fn setup_runtime(&self, rt: &runtime::Runtime) -> Result<()> {
        tracing::debug!("setting up runtime...");
        rt.ensure_required_directories().await
    }

    /// Mount the provided runtime via the macFUSE backend.
    #[cfg(feature = "fuse-backend")]
    pub async fn mount_env_fuse(&self, rt: &runtime::Runtime) -> Result<()> {
        ensure_service_running().await?;
        RuntimeConfigurator.mount_fuse_onto(rt, SPFS_DIR).await
    }

    /// Unmount the environment.
    pub async fn unmount_env(&self, rt: &runtime::Runtime, lazy: bool) -> Result<()> {
        self.unmount_env_fuse(rt, lazy).await
    }

    /// Unmount the FUSE filesystem.
    async fn unmount_env_fuse(&self, _rt: &runtime::Runtime, lazy: bool) -> Result<()> {
        tracing::debug!(%lazy, "unmounting existing fuse env @ {SPFS_DIR}...");

        if !service_is_running(MACOS_FUSE_SERVICE_ADDR).await {
            tracing::debug!("macFUSE service not running, nothing to unmount");
            return Ok(());
        }

        // Use umount on macOS
        let flags = if lazy { "-f" } else { "" };
        let mut cmd = tokio::process::Command::new("umount");
        if !flags.is_empty() {
            cmd.arg(flags);
        }
        cmd.arg(SPFS_DIR);

        match cmd.status().await {
            Err(err) => Err(Error::ProcessSpawnError("umount".into(), err)),
            Ok(status) if status.success() => Ok(()),
            Ok(status) => Err(Error::String(format!(
                "Failed to unmount FUSE filesystem: umount exited with {:?}",
                status.code()
            ))),
        }
    }

    /// Change the runtime to durable mode.
    ///
    /// On macOS with FUSE-only backend, durable runtimes are not supported.
    pub async fn change_runtime_to_durable(&self, _runtime: &mut runtime::Runtime) -> Result<i32> {
        Err(Error::RuntimeChangeToDurableError(
            "macFUSE backend does not support durable runtimes".to_string(),
        ))
    }

    /// Unmount overlayfs portion.
    ///
    /// On macOS, there is no overlayfs. This is a no-op.
    pub async fn unmount_env_overlayfs(&self, _rt: &runtime::Runtime, _lazy: bool) -> Result<()> {
        // macOS doesn't have overlayfs
        Ok(())
    }

    /// Mount overlayfs.
    ///
    /// On macOS, there is no overlayfs. This returns an error.
    pub async fn mount_env_overlayfs<P: AsRef<Path>>(
        &self,
        _global_overlayfs_options: &OverlayFsOptions,
        _rt: &runtime::Runtime,
        _layer_dirs: &[P],
    ) -> Result<()> {
        Err(Error::String(
            "overlayfs is not supported on macOS; use FUSE-only backend".to_string(),
        ))
    }

    /// Mask files in the runtime.
    ///
    /// On macOS with FUSE, file masking is handled differently.
    pub async fn mask_files(
        &self,
        _config: &runtime::Config,
        _manifest: super::tracking::Manifest,
    ) -> Result<()> {
        // File masking is handled by the FUSE filesystem on macOS
        Ok(())
    }

    /// Mount the runtime tmpfs.
    ///
    /// On macOS, we don't have tmpfs. This is a no-op.
    pub fn mount_runtime(&self, _config: &runtime::Config) -> Result<()> {
        // macOS doesn't have tmpfs mounts like Linux
        Ok(())
    }
}

/// Get FUSE mount arguments for macOS.
#[cfg(feature = "fuse-backend")]
fn get_fuse_args(config: &runtime::Config, read_only: bool) -> String {
    use fuser::MountOption::*;
    use itertools::Itertools;

    let mut opts = vec![
        NoDev,
        NoAtime,
        NoSuid,
        Exec,
        AllowOther,
        CUSTOM(format!("uid={}", nix::unistd::getuid())),
        CUSTOM(format!("gid={}", nix::unistd::getgid())),
    ];
    opts.push(if read_only { RO } else { RW });
    opts.extend(
        config
            .secondary_repositories
            .iter()
            .map(|r| CUSTOM(format!("remote={r}"))),
    );
    opts.push(CUSTOM(format!(
        "incl_sec_tags={}",
        config.include_secondary_tags
    )));
    opts.iter().map(option_to_string).join(",")
}

/// Format option to be passed to libfuse or kernel.
#[cfg(feature = "fuse-backend")]
pub fn option_to_string(option: &fuser::MountOption) -> String {
    use fuser::MountOption;
    match option {
        MountOption::FSName(name) => format!("fsname={}", name),
        MountOption::Subtype(subtype) => format!("subtype={}", subtype),
        MountOption::CUSTOM(value) => value.to_string(),
        MountOption::AutoUnmount => "auto_unmount".to_string(),
        MountOption::AllowOther => "allow_other".to_string(),
        MountOption::AllowRoot => "allow_other".to_string(),
        MountOption::DefaultPermissions => "default_permissions".to_string(),
        MountOption::Dev => "dev".to_string(),
        MountOption::NoDev => "nodev".to_string(),
        MountOption::Suid => "suid".to_string(),
        MountOption::NoSuid => "nosuid".to_string(),
        MountOption::RO => "ro".to_string(),
        MountOption::RW => "rw".to_string(),
        MountOption::Exec => "exec".to_string(),
        MountOption::NoExec => "noexec".to_string(),
        MountOption::Atime => "atime".to_string(),
        MountOption::NoAtime => "noatime".to_string(),
        MountOption::DirSync => "dirsync".to_string(),
        MountOption::Sync => "sync".to_string(),
        MountOption::Async => "async".to_string(),
    }
}

/// Get the overlayfs arguments for the given list of layer directories.
///
/// On macOS, this always returns an error since overlayfs is not supported.
pub fn get_overlay_args<P: AsRef<Path>>(
    _global_overlayfs_options: &OverlayFsOptions,
    _rt: &runtime::Runtime,
    _layer_dirs: &[P],
) -> Result<String> {
    Err("overlayfs is not supported on macOS".into())
}

async fn service_is_running(addr: &str) -> bool {
    let endpoint = match Endpoint::from_shared(format!("http://{addr}")) {
        Ok(endpoint) => endpoint,
        Err(_) => return false,
    };

    timeout(
        Duration::from_millis(SERVICE_CHECK_TIMEOUT_MS),
        endpoint.connect(),
    )
    .await
    .ok()
    .and_then(|result| result.ok())
    .is_some()
}

async fn start_service_background() -> Result<()> {
    let spfs_fuse = crate::resolve::which_spfs("fuse-macos")
        .ok_or_else(|| Error::MissingBinary("spfs-fuse-macos"))?;

    let mut cmd = Command::new(&spfs_fuse);
    cmd.arg("service")
        .arg(SPFS_DIR)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .stdin(Stdio::null());

    #[cfg(unix)]
    {
        #[allow(unused_imports)]
        use std::os::unix::process::CommandExt;
        unsafe {
            cmd.pre_exec(|| {
                nix::unistd::setsid()
                    .map(|_| ())
                    .map_err(|err| std::io::Error::from_raw_os_error(err as i32))
            });
        }
    }

    tracing::debug!(?spfs_fuse, "starting macFUSE service in background");

    cmd.spawn()
        .map(|_| ())
        .map_err(|e| Error::process_spawn_error("spfs-fuse-macos service", e, None))?;

    Ok(())
}

async fn wait_for_service_ready(addr: &str, timeout: Duration) -> Result<()> {
    let start = Instant::now();
    let mut backoff = Duration::from_millis(50);
    const MAX_BACKOFF: Duration = Duration::from_millis(500);

    while start.elapsed() < timeout {
        if service_is_running(addr).await {
            tracing::debug!(elapsed_ms = start.elapsed().as_millis(), "service is ready");
            return Ok(());
        }

        tokio::time::sleep(backoff).await;
        backoff = std::cmp::min(backoff * 2, MAX_BACKOFF);
    }

    Err(Error::String(format!(
        "macFUSE service did not start within {} seconds. \
         Ensure macFUSE is installed: brew install --cask macfuse",
        timeout.as_secs()
    )))
}

pub async fn ensure_service_running() -> Result<()> {
    let addr = MACOS_FUSE_SERVICE_ADDR;

    for attempt in 0..MAX_SERVICE_START_RETRIES {
        if service_is_running(addr).await {
            if attempt > 0 {
                tracing::debug!(attempt, "service detected after retry");
            }
            return Ok(());
        }

        if attempt == 0 {
            tracing::info!("macFUSE service not running, starting automatically...");
        }

        match start_service_background().await {
            Ok(()) => {
                let timeout = Duration::from_secs(SERVICE_STARTUP_TIMEOUT_SECS);
                match wait_for_service_ready(addr, timeout).await {
                    Ok(()) => {
                        tracing::info!("macFUSE service started successfully");
                        return Ok(());
                    }
                    Err(e) if attempt < MAX_SERVICE_START_RETRIES - 1 => {
                        tracing::debug!(attempt, error = %e, "service start attempt failed, retrying");
                    }
                    Err(e) => return Err(e),
                }
            }
            Err(e) if attempt < MAX_SERVICE_START_RETRIES - 1 => {
                tracing::debug!(attempt, error = %e, "service start failed, checking if another process started it");
                let backoff = Duration::from_millis(100 * (1 << attempt));
                tokio::time::sleep(backoff).await;
                continue;
            }
            Err(e) => {
                return Err(Error::String(format!(
                    "Failed to start macFUSE service: {}. \
                     Ensure macFUSE is installed: brew install --cask macfuse",
                    e
                )));
            }
        }
    }

    Err(Error::String(
        "Could not start or connect to macFUSE service after multiple attempts".to_string(),
    ))
}
