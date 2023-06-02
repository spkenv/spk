// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

//! Functions related to the monitoring of an active spfs runtime
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use dashmap::DashSet;
use tokio_retry::strategy::{jitter, ExponentialBackoff};
use tokio_retry::{Condition, RetryIf};
use tokio_stream::wrappers::{IntervalStream, UnboundedReceiverStream};
use tokio_stream::StreamExt;

use super::runtime;
use crate::repeating_timeout::RepeatingTimeout;
use crate::{Error, Result};

pub const PROC_DIR: &str = "/proc";

pub const SPFS_MONITOR_FOREGROUND_LOGGING_VAR: &str = "SPFS_MONITOR_FOREGROUND_LOGGING";
const SPFS_MONITOR_DISABLE_CNPROC_VAR: &str = "SPFS_MONITOR_DISABLE_CNPROC";

/// Run an spfs monitor for the provided runtime
///
/// The monitor command will spawn but immediately fail
/// if there is already a monitor registered to this runtime
pub fn spawn_monitor_for_runtime(rt: &runtime::Runtime) -> Result<tokio::process::Child> {
    let exe = match super::resolve::which_spfs("monitor") {
        None => return Err(Error::MissingBinary("spfs")),
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

    unsafe {
        // Avoid creating zombie processes by moving the monitor into a
        // separate process group. Use `daemon` to reparent it to pid 1 in
        // order to avoid the monitor being discovered by walking the
        // process tree and being killed by aggressive process management,
        // like in a render farm job situation.
        cmd.pre_exec(|| match nix::unistd::daemon(false, true) {
            Ok(_pid) => Ok(()),
            Err(err) => Err(std::io::Error::from_raw_os_error(err as i32)),
        });
    }

    cmd.spawn()
        .map_err(|err| Error::process_spawn_error("spfs-monitor".to_owned(), err, None))
}

/// When provided an active runtime, wait until all contained processes exit
///
/// This is a privileged operation that may fail with a permission
/// issue if the calling process is not root or CAP_NET_ADMIN
pub async fn wait_for_empty_runtime(rt: &runtime::Runtime) -> Result<()> {
    let pid = match rt.status.owner {
        None => return Err(Error::RuntimeNotInitialized(rt.name().into())),
        Some(pid) => pid,
    };

    // Grab the mount namespace as soon as possible before the pid has a
    // chance to disappear.

    // This was observed to require two retries before succeeding when set to
    // 10ms.
    let retry_strategy = ExponentialBackoff::from_millis(50).map(jitter).take(3);

    struct RetryOnPermissionDenied {
        had_to_retry: Arc<AtomicBool>,
    }

    impl Condition<Error> for RetryOnPermissionDenied {
        fn should_retry(&mut self, error: &Error) -> bool {
            #[cfg(feature = "sentry")]
            tracing::info!(target: "sentry", ?error, "In should_retry after identify_mount_namespace_of_process");

            // Only retry if the namespace couldn't be read because of EACCES.
            match error {
                Error::RuntimeReadError(_, err)
                    if matches!(err.kind(), std::io::ErrorKind::PermissionDenied) =>
                {
                    self.had_to_retry.store(true, Ordering::Relaxed);

                    true
                }
                _ => false,
            }
        }
    }

    let had_to_retry = Arc::new(AtomicBool::new(false));

    let mount_ns = RetryIf::spawn(
        retry_strategy,
        || async { identify_mount_namespace_of_process(pid).await },
        RetryOnPermissionDenied {
            had_to_retry: Arc::clone(&had_to_retry),
        },
    )
    .await
    .map(|result| {
        if had_to_retry.load(Ordering::Relaxed) {
            // Want to know if this happened...
            #[cfg(feature = "sentry")]
            tracing::error!(target: "sentry", "read mount namespace succeeded after retries");
        }

        result
    })
    .unwrap_or_default();

    // we could use our own pid here, but then when running in a container
    // or pid namespace the process id would actually be wrong. Passing
    // zero will get the kernel to determine our pid from its perspective
    let monitor = match is_cnproc_disabled() {
        false => cnproc::PidMonitor::from_id(0)
            .map_err(|e| crate::Error::String(format!("failed to establish process monitor: {e}"))),
        true => Err(format!("cnproc disabled by env: {SPFS_MONITOR_DISABLE_CNPROC_VAR}").into()),
    };

    let mut tracked_processes = HashSet::new();
    let (events_send, events_recv) = tokio::sync::mpsc::unbounded_channel();

    // NOTE(rbottriell):
    // scan for any processes that were already spawned before
    // we had setup the monitoring socket, this will establish
    // the initial set of parent processes that we can track the
    // lineage of
    // This explicitly happens AFTER setting up the monitor so that
    // we will receive news of any new process created out of whatever
    // we find in this scan
    // BUG(rbottriell) There is still a race condition here where a
    // child is created as we scan, and the parent exits before we
    // are able to see it so we don't think the child is relevant
    let current_pids = match mount_ns.as_ref() {
        Some(ns) => match find_other_processes_in_mount_namespace(ns).await {
            Err(err) => {
                tracing::error!(?err, "error while scanning active process tree");
                return Err(err);
            }
            Ok(pids) => pids,
        },
        None => HashSet::new(),
    };
    tracked_processes.extend(current_pids);

    // it's possible that the runtime process(es)
    // completed before we were even able to see them
    if tracked_processes.is_empty() {
        tracing::info!("no processes to track, monitor must exit");
        return Ok(());
    }

    match (monitor, mount_ns.clone()) {
        (Ok(mut monitor), _) => {
            tokio::task::spawn_blocking(move || {
                // Normal behavior when the monitor was successfully created.
                while let Some(event) = monitor.recv() {
                    if let Err(_err) = events_send.send(event) {
                        // the receiver has stopped listening, no need to continue
                        return;
                    }
                }
                tracing::warn!("monitor event stream ended unexpectedly");
            });
        }
        (Err(err), Some(ns)) => {
            let mut tracked_processes = tracked_processes.clone();

            tokio::task::spawn(async move {
                // Fallback to polling the process tree.

                tracing::info!(?err, "Process monitor failed; using fallback mechanism");

                // No need to poll rapidly; we don't care about short-lived
                // processes and only need to find at least one existing
                // process to keep the runtime alive.
                const PROC_POLLING_INTERVAL: std::time::Duration =
                    std::time::Duration::from_millis(2500);
                let interval = tokio::time::interval(PROC_POLLING_INTERVAL);
                let mut interval_stream = IntervalStream::new(interval);

                while !tracked_processes.is_empty() && interval_stream.next().await.is_some() {
                    let current_pids = match find_other_processes_in_mount_namespace(&ns).await {
                        Err(err) => {
                            tracing::error!(?err, "error while scanning active process tree");
                            break;
                        }
                        Ok(pids) => pids,
                    };

                    // Grab one of the existing pids to play the role of
                    // parent for any new pid.
                    let parent = *tracked_processes.iter().next().unwrap();

                    for new_pid in current_pids.difference(&tracked_processes) {
                        if let Err(_err) = events_send.send(cnproc::PidEvent::Fork {
                            parent: parent as i32,
                            pid: *new_pid as i32,
                        }) {
                            // the receiver has stopped listening, no need to continue
                            return;
                        }
                    }

                    for expiring_pid in tracked_processes.difference(&current_pids) {
                        if let Err(_err) =
                            events_send.send(cnproc::PidEvent::Exit(*expiring_pid as i32))
                        {
                            // the receiver has stopped listening, no need to continue
                            return;
                        }
                    }

                    tracked_processes = current_pids;
                }
            });
        }
        (pid_monitor_result, mount_ns) => {
            // No way to monitor; give up.
            // Let receiver know there won't be any messages.
            drop(events_send);

            tracing::warn!(
                ?pid_monitor_result,
                ?mount_ns,
                "no way to monitor runtime; it will be deleted immediately!"
            );
        }
    }

    let tracked_processes = Arc::new(
        DashSet::<_, std::collections::hash_map::RandomState>::from_iter(
            tracked_processes.into_iter(),
        ),
    );

    let events_stream = UnboundedReceiverStream::new(events_recv);

    // Filter the stream to keep only the events that pertain to us
    let events_stream = {
        let tracked_processes = Arc::clone(&tracked_processes);

        events_stream.filter(move |event| match event {
            cnproc::PidEvent::Exec(_pid) => {
                // exec is just one process turning into a new one with
                // the pid remaining the same, so we are not interested...
                // remember that launching a new process is a fork and then exec
                false
            }
            cnproc::PidEvent::Fork { parent, pid }
                if tracked_processes.contains(&(*parent as u32)) =>
            {
                tracked_processes.insert(*pid as u32);
                true
            }
            cnproc::PidEvent::Fork { .. } => false,
            cnproc::PidEvent::Exit(pid) if tracked_processes.remove(&(*pid as u32)).is_some() => {
                true
            }
            cnproc::PidEvent::Exit(..) => false,
            cnproc::PidEvent::Coredump(pid) => {
                tracing::trace!(?tracked_processes, ?pid, "notified of core dump");
                false
            }
        })
    };

    // Add a timeout to be able to detect when no relevant pid events are
    // arriving for a period of time.
    let events_stream = RepeatingTimeout::new(events_stream, tokio::time::Duration::from_secs(60));
    tokio::pin!(events_stream);

    async fn repair_tracked_processes<S>(tracked_processes: &Arc<DashSet<u32, S>>, ns: &Path)
    where
        S: std::hash::BuildHasher + Clone,
    {
        let current_pids = match find_other_processes_in_mount_namespace(ns).await {
            Err(err) => {
                tracing::error!(?err, "error while scanning active process tree");
                return;
            }
            Ok(pids) => pids,
        };

        // `tracked_processes` is a concurrent set; take care to not leave it
        // falsely empty at any point.

        // If `current_pids` is empty, then we can efficiently clear our set.
        if current_pids.is_empty() {
            tracked_processes.clear();
            return;
        }

        for pid in &current_pids {
            tracked_processes.insert(*pid);
        }

        tracked_processes.retain(|key| current_pids.contains(key));
    }

    const LOG_UPDATE_INTERVAL: tokio::time::Duration = tokio::time::Duration::from_secs(5);
    let mut log_update_deadline = tokio::time::Instant::now() + LOG_UPDATE_INTERVAL;

    while let Some(event) = events_stream.next().await {
        let no_more_processes = tracked_processes.is_empty();

        // Limit how often the process set is logged, but we want to always
        // log when `tracked_processes` becomes empty, no matter when that
        // happens.
        let now = tokio::time::Instant::now();
        if now >= log_update_deadline || no_more_processes {
            tracing::trace!(?tracked_processes, "runtime monitor");
            log_update_deadline = now + LOG_UPDATE_INTERVAL;
        }

        if no_more_processes {
            // If the mount namespace is known, verify there really aren't any
            // more processes in the namespace. Since cnproc might not deliver
            // every event, we may not know about processes that still exist.
            if let Some(ns) = mount_ns.as_ref() {
                repair_tracked_processes(&tracked_processes, ns).await;

                if tracked_processes.is_empty() {
                    // Confirmed there is none left.
                    break;
                }

                // Log if this happens, out of curiosity to see if this ever
                // actually happens in practice.
                tracing::info!(?tracked_processes, "Discovered new pids after repairing");
            } else {
                // Have to trust cnproc...
                break;
            }
        }

        // Check for any timeouts...
        if event.is_err() {
            tracing::trace!(?tracked_processes, "no pid events for a while");

            // If the mount namespace is known, repair `tracked_processes` by
            // walking the process tree. This will remove any pids that we
            // didn't get notified about exiting.
            if let Some(ns) = mount_ns.as_ref() {
                repair_tracked_processes(&tracked_processes, ns).await;
                tracing::trace!(?tracked_processes, "repaired");
            }
        }
    }
    Ok(())
}

fn is_cnproc_disabled() -> bool {
    match std::env::var(SPFS_MONITOR_DISABLE_CNPROC_VAR) {
        Err(_) => false,
        Ok(s) if s == "0" => false,
        Ok(_) => true,
    }
}

/// Identify the mount namespace of the provided process id.
///
/// Return None if the pid is not found.
pub async fn identify_mount_namespace_of_process(pid: u32) -> Result<Option<std::path::PathBuf>> {
    let ns_path = std::path::Path::new(PROC_DIR)
        .join(pid.to_string())
        .join("ns/mnt");

    tracing::debug!(?ns_path, "Getting process namespace");
    match tokio::fs::read_link(&ns_path).await {
        Ok(ns) => Ok(Some(ns)),
        Err(err) => match err.kind() {
            std::io::ErrorKind::NotFound => {
                // it's possible that the runtime process already exited
                // or was never started
                tracing::debug!(
                    ?ns_path,
                    "runtime process appears to no longer exists or was never started"
                );
                Ok(None)
            }
            _ => Err(Error::RuntimeReadError(ns_path, err)),
        },
    }
}

/// Return an inventory of all known pids and their mount namespaces.
pub async fn find_processes_and_mount_namespaces() -> Result<HashMap<u32, Option<PathBuf>>> {
    let mut found_processes = HashMap::new();

    let mut read_dir = tokio::fs::read_dir(PROC_DIR)
        .await
        .map_err(|err| Error::RuntimeReadError(PROC_DIR.into(), err))?;
    while let Some(entry) = read_dir
        .next_entry()
        .await
        .map_err(|err| Error::RuntimeReadError(PROC_DIR.into(), err))?
    {
        let pid = match entry.file_name().to_str().map(|s| s.parse::<u32>()) {
            Some(Ok(pid)) => pid,
            // don't bother reading proc dirs that are not named with a valid pid
            _ => continue,
        };
        let link_path = entry.path().join("ns/mnt");
        let found_ns = tokio::fs::read_link(&link_path).await.ok();
        found_processes.insert(pid, found_ns);
    }
    Ok(found_processes)
}

/// Provided the namespace symlink content from /proc fs,
/// scan for all processes that share the same namespace
///
/// This function explicitly excludes the current pid, because
/// the monitor does not consider itself important when determining
/// whether a runtime is empty and should shut down (otherwise it
/// never would)
async fn find_other_processes_in_mount_namespace(ns: &std::path::Path) -> Result<HashSet<u32>> {
    let mut found_processes = HashSet::new();

    let mut read_dir = tokio::fs::read_dir(PROC_DIR)
        .await
        .map_err(|err| Error::RuntimeReadError(PROC_DIR.into(), err))?;
    while let Some(entry) = read_dir
        .next_entry()
        .await
        .map_err(|err| Error::RuntimeReadError(PROC_DIR.into(), err))?
    {
        let pid = match entry.file_name().to_str().map(|s| s.parse::<u32>()) {
            Some(Ok(pid)) => pid,
            // don't bother reading proc dirs that are not named with a valid pid
            _ => continue,
        };
        let link_path = entry.path().join("ns/mnt");
        let found_ns = match tokio::fs::read_link(&link_path).await {
            Ok(p) => p,
            Err(err) => match err.raw_os_error() {
                Some(libc::ENOENT) => continue,
                Some(libc::ENOTDIR) => continue,
                Some(libc::EACCES) => continue,
                Some(libc::EPERM) => continue,
                _ => {
                    return Err(Error::RuntimeReadError(link_path, err));
                }
            },
        };
        if found_ns == ns {
            found_processes.insert(pid);
        }
    }
    found_processes.remove(&(nix::unistd::getpid().as_raw() as u32));
    Ok(found_processes)
}
