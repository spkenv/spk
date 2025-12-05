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

fn ensure_spfs_mountpoint_exists() -> Result<()> {
    let path = Path::new(SPFS_DIR);
    if path.is_dir() {
        return Ok(());
    }

    Err(Error::String(format!(
        "SPFS mount point {} does not exist. On macOS, you must configure /etc/synthetic.conf (e.g. via 'make -f Makefile.macos setup-spfs-mount') and reboot if required.",
        SPFS_DIR
    )))
}

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
    /// On macOS with FUSE, we don't actually need root privileges since
    /// the FUSE service runs in userspace. This function simply returns
    /// a RootConfigurator without changing privileges.
    pub fn become_root(self) -> Result<RootConfigurator> {
        tracing::debug!("become_root called (no-op on macOS with FUSE)");
        let original_uid = nix::unistd::getuid();
        let original_euid = nix::unistd::geteuid();
        Ok(RootConfigurator {
            original_uid,
            original_euid,
        })
    }

    /// Mount the provided runtime via the macFUSE backend.
    pub async fn mount_env_fuse(&self, rt: &runtime::Runtime) -> Result<()> {
        ensure_service_running().await?;
        ensure_spfs_mountpoint_exists()?;
        self.mount_fuse_onto(rt, SPFS_DIR).await
    }

    async fn mount_fuse_onto<P>(&self, rt: &runtime::Runtime, path: P) -> Result<()>
    where
        P: AsRef<std::ffi::OsStr>,
    {
        use spfs_encoding::prelude::*;

        let path = path.as_ref().to_owned();
        let platform = rt.to_platform().digest()?.to_string();
        let editable = rt.status.editable;
        let read_only = !editable;

        // Build mount options for macFUSE (currently unused as we delegate to CLI)
        let _opts = get_fuse_args(&rt.config, read_only);

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
        // For editable mounts, use the runtime owner's PID (the shell process that
        // will be writing to the filesystem). For read-only mounts, use the current
        // process PID. This is important because spfs-enter --remount is a transient
        // process that exits immediately, but the runtime owner (shell) keeps running.
        let root_pid = if editable {
            rt.status.owner.unwrap_or_else(std::process::id)
        } else {
            std::process::id()
        };
        cmd.arg("--root-process").arg(root_pid.to_string());
        cmd.arg(&platform);
        // Capture stderr to see mount errors
        cmd.stdout(std::process::Stdio::null());
        cmd.stderr(std::process::Stdio::piped());
        tracing::debug!("Running mount command: {cmd:?}");

        let output = cmd
            .output()
            .map_err(|err| Error::process_spawn_error("mount", err, None))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::String(format!(
                "Failed to mount fuse filesystem, mount command exited with status {:?}. stderr: {}",
                output.status.code(),
                stderr
            )));
        }

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
    ///
    /// On macOS with FUSE, we never actually escalated privileges,
    /// so this is a no-op.
    pub fn become_original_user(self) -> Result<RuntimeConfigurator> {
        tracing::debug!("become_original_user called (no-op on macOS with FUSE)");
        Ok(RuntimeConfigurator)
    }

    /// Remove mount propagation.
    ///
    /// On macOS, there is no mount propagation to remove. This is a no-op.
    pub async fn remove_mount_propagation(&self) -> Result<()> {
        // macOS doesn't have mount propagation
        Ok(())
    }

    /// Check the necessary directories for mounting the provided runtime.
    pub fn ensure_mount_targets_exist(&self, _config: &runtime::Config) -> Result<()> {
        tracing::debug!("ensuring mount targets exist...");
        ensure_spfs_mountpoint_exists()
    }

    /// Setup runtime directories.
    pub async fn setup_runtime(&self, rt: &runtime::Runtime) -> Result<()> {
        tracing::debug!("setting up runtime...");
        rt.ensure_required_directories().await
    }

    /// Mount the provided runtime via the macFUSE backend.
    pub async fn mount_env_fuse(&self, rt: &runtime::Runtime) -> Result<()> {
        ensure_service_running().await?;
        ensure_spfs_mountpoint_exists()?;
        RuntimeConfigurator.mount_fuse_onto(rt, SPFS_DIR).await
    }

    /// Unmount the environment.
    pub async fn unmount_env(&self, rt: &runtime::Runtime, lazy: bool) -> Result<()> {
        self.unmount_env_fuse(rt, lazy).await
    }

    /// Unmount the FUSE filesystem route for a process tree.
    ///
    /// This doesn't actually unmount the FUSE filesystem (which is shared),
    /// but tells the router to remove the route for this runtime's owner.
    async fn unmount_env_fuse(&self, rt: &runtime::Runtime, _lazy: bool) -> Result<()> {
        tracing::debug!("unmounting fuse env route for runtime...");

        if !service_is_running(MACOS_FUSE_SERVICE_ADDR).await {
            tracing::debug!("macFUSE service not running, nothing to unmount");
            return Ok(());
        }

        // Get the PID to unmount - prefer the runtime owner, fall back to current process
        let root_pid = rt.status.owner.unwrap_or_else(std::process::id);

        // Tell the service to unmount this PID's route
        let spfs_fuse = match super::resolve::which_spfs("fuse-macos") {
            None => return Err(Error::MissingBinary("spfs-fuse-macos")),
            Some(exe) => exe,
        };

        let mut cmd = std::process::Command::new(spfs_fuse);
        cmd.arg("unmount").arg(root_pid.to_string());
        tracing::debug!("Running unmount command: {cmd:?}");

        let output = cmd
            .output()
            .map_err(|err| Error::process_spawn_error("unmount", err, None))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // Don't fail if there was nothing to unmount
            if stderr.contains("No mount found") {
                tracing::debug!("No existing mount found for PID {root_pid}");
                return Ok(());
            }
            return Err(Error::String(format!(
                "Failed to unmount fuse route: spfs-fuse-macos unmount exited with {:?}. stderr: {}",
                output.status.code(),
                stderr
            )));
        }

        tracing::debug!(%root_pid, "Unmounted fuse route");
        Ok(())
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
///
/// Returns a comma-separated string of mount options to pass to macFUSE.
fn get_fuse_args(config: &runtime::Config, read_only: bool) -> String {
    let mut opts = vec![
        "nodev".to_string(),
        "noatime".to_string(),
        "nosuid".to_string(),
        "exec".to_string(),
        "allow_other".to_string(),
        format!("uid={}", nix::unistd::getuid()),
        format!("gid={}", nix::unistd::getgid()),
    ];
    opts.push(if read_only { "ro" } else { "rw" }.to_string());
    for repo in &config.secondary_repositories {
        opts.push(format!("remote={repo}"));
    }
    opts.push(format!("incl_sec_tags={}", config.include_secondary_tags));
    opts.join(",")
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
