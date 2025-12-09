// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

//! macOS implementation of runtime monitoring using kqueue process tracking

use std::time::Duration;

use tokio::time::{Instant, interval};
use tokio_stream::StreamExt;
use tokio_stream::wrappers::IntervalStream;

use super::runtime;
use crate::{Error, Result};

#[cfg(target_os = "macos")]
use crate::process::{ProcessWatcher, is_in_process_tree};

pub const SPFS_MONITOR_FOREGROUND_LOGGING_VAR: &str = "SPFS_MONITOR_FOREGROUND_LOGGING";

/// Run an spfs monitor for the provided runtime
///
/// The monitor command will spawn but immediately fail
/// if there is already a monitor registered to this runtime
pub fn spawn_monitor_for_runtime(rt: &runtime::Runtime) -> Result<tokio::process::Child> {
    if rt.config.mount_backend.is_winfsp() {
        todo!("No monitor implementation for winfsp mounts");
    }

    let exe = match super::resolve::which_spfs("monitor") {
        None => return Err(Error::MissingBinary("spfs-monitor")),
        Some(exe) => exe,
    };

    let mut cmd = tokio::process::Command::new(exe);
    cmd.arg("--runtime-storage");
    cmd.arg(rt.storage().address().as_str());
    cmd.arg("--runtime");
    cmd.arg(rt.name());
    // the monitor process should be fully detached from any controlling
    // terminal. Otherwise, using spfs run under output-capturing circumstances
    // can cause the command to hang forever. Eg: output=$(spfs run - -- echo "hello")
    cmd.stdout(std::process::Stdio::null());
    // however, we need to communicate with the monitor process to tell it when
    // it is able to read our mount namespace, once we've established it and
    // dropped privs.
    cmd.stdin(std::process::Stdio::piped());
    // However, being able to see the logs is valuable when debugging, and so
    // we add a switch to enable this if desired
    if std::env::var(SPFS_MONITOR_FOREGROUND_LOGGING_VAR).is_err() {
        cmd.stderr(std::process::Stdio::null());
    }

    #[cfg(target_os = "macos")]
    unsafe {
        // On macOS, use setsid to create a new session and detach from
        // the controlling terminal. This achieves similar process isolation
        // as daemon() on Linux.
        cmd.pre_exec(|| {
            nix::unistd::setsid()
                .map(|_| ())
                .map_err(|err| std::io::Error::from_raw_os_error(err as i32))
        });
    }

    cmd.spawn()
        .map_err(|err| Error::process_spawn_error("spfs-monitor", err, None))
}

/// When provided an active runtime, wait until all contained processes exit
///
/// This function tracks the root process and its descendants using kqueue
/// on macOS, since macOS doesn't have mount namespaces like Linux.
pub async fn wait_for_empty_runtime(rt: &runtime::Runtime, config: &crate::Config) -> Result<()> {
    let root_pid = match rt.status.owner {
        None => return Err(Error::RuntimeNotInitialized(rt.name().into())),
        Some(pid) => pid,
    };

    tracing::debug!(pid = root_pid, "starting macOS process monitor");

    // Initialize process watcher for macOS
    let mut watcher =
        ProcessWatcher::new().map_err(|e| Error::RuntimeReadError("kqueue".into(), e))?;

    // Try to watch the root process
    if !watcher
        .watch(root_pid)
        .map_err(|e| Error::RuntimeReadError("kqueue watch".into(), e))?
    {
        // Root process already exited
        tracing::debug!(pid = root_pid, "root process already exited");
        return Ok(());
    }

    const PROC_POLLING_INTERVAL: Duration = Duration::from_millis(2500);
    let mut interval_stream = IntervalStream::new(interval(PROC_POLLING_INTERVAL));

    const LOG_UPDATE_INTERVAL: Duration = Duration::from_secs(5);
    let mut log_update_deadline = Instant::now() + LOG_UPDATE_INTERVAL;

    let spfs_heartbeat_interval: Duration =
        Duration::from_secs(config.fuse.heartbeat_interval_seconds.get());
    let mut spfs_heartbeat_deadline = Instant::now() + spfs_heartbeat_interval;
    let enable_heartbeat = config.fuse.enable_heartbeat && rt.is_backend_fuse();

    loop {
        // Check for process exit with a short timeout
        match watcher.wait_for_exit(Duration::from_secs(1)) {
            Ok(Some(exited_pid)) => {
                tracing::debug!(pid = exited_pid, "process exited via kqueue");
                if exited_pid == root_pid {
                    // Root process exited, check if any descendants remain
                    let mut has_descendants = false;
                    // Scan for descendant processes
                    for pid in 1..=100000 {
                        if pid as u32 == root_pid {
                            continue;
                        }
                        if ProcessWatcher::is_process_alive(pid as u32)
                            && is_in_process_tree(pid, root_pid as i32)
                        {
                            tracing::debug!(descendant_pid = pid, "found living descendant");
                            has_descendants = true;
                            // Start watching this descendant
                            let _ = watcher.watch(pid as u32);
                        }
                    }

                    if !has_descendants {
                        tracing::debug!("root process and all descendants exited");
                        break;
                    }
                }
            }
            Ok(None) => {
                // Timeout, check if root process is still alive
                if !ProcessWatcher::is_process_alive(root_pid) {
                    tracing::debug!(
                        pid = root_pid,
                        "root process no longer alive, checking descendants"
                    );

                    // Check if any descendant processes are still alive
                    let mut has_descendants = false;
                    for pid in 1..=100000 {
                        if pid as u32 == root_pid {
                            continue;
                        }
                        if ProcessWatcher::is_process_alive(pid as u32)
                            && is_in_process_tree(pid, root_pid as i32)
                        {
                            tracing::debug!(descendant_pid = pid, "found living descendant");
                            has_descendants = true;
                            break;
                        }
                    }

                    if !has_descendants {
                        tracing::debug!("root process and all descendants exited");
                        break;
                    }
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "error waiting for process exit");
            }
        }

        // Check for interval tick for logging and heartbeat
        tokio::select! {
            _ = interval_stream.next() => {
                // Log updates
                let now = Instant::now();
                if now >= log_update_deadline {
                    tracing::trace!(pid = root_pid, "runtime monitor still waiting");
                    log_update_deadline = now + LOG_UPDATE_INTERVAL;
                }

                // Heartbeat for FUSE filesystem
                if enable_heartbeat && now >= spfs_heartbeat_deadline {
                    // Tickle the spfs filesystem to let `spfs-fuse` know we're still
                    // alive. This is a read operation to avoid issues with ro mounts
                    // or modifying any content in /spfs.
                    // The filename has a unique component to avoid any caching.
                    let _ = tokio::fs::symlink_metadata(format!(
                        "/spfs/{}{}",
                        crate::config::Fuse::HEARTBEAT_FILENAME_PREFIX,
                        ulid::Ulid::new()
                    ))
                    .await;

                    spfs_heartbeat_deadline = now + spfs_heartbeat_interval;
                }
            }
            _ = tokio::time::sleep(Duration::from_millis(100)) => {
                // Small sleep to prevent busy loop
            }
        }
    }

    Ok(())
}

/// Identify the mount namespace of the provided process id.
///
/// On macOS, mount namespaces don't exist, so we always return None.
pub async fn identify_mount_namespace_of_process(_pid: u32) -> Result<Option<std::path::PathBuf>> {
    // macOS doesn't have mount namespaces in /proc like Linux
    Ok(None)
}

/// Return an inventory of all known pids and their mount namespaces.
///
/// On macOS, mount namespaces don't exist, so we return an empty map.
pub async fn find_processes_and_mount_namespaces()
-> Result<std::collections::HashMap<u32, Option<std::path::PathBuf>>> {
    // macOS doesn't have mount namespaces
    Ok(std::collections::HashMap::new())
}

