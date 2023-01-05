// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use chrono::{Duration, Utc};
use clap::Args;
use tokio_stream::StreamExt;

/// List runtime information from the repository
#[derive(Debug, Args)]
#[clap(visible_alias = "prune")]
pub struct CmdRuntimePrune {
    /// Prune a runtime in a remote or alternate repository
    #[clap(short, long)]
    remote: Option<String>,

    /// Remove the runtime even if it's owned by someone else
    #[clap(long)]
    ignore_user: bool,

    /// Remove the runtime even if it appears to be from a different host
    ///
    /// Implies --ignore-monitor
    #[clap(long)]
    ignore_host: bool,

    /// Do not try and terminate the monitor process, just remove runtime data
    #[clap(long)]
    ignore_monitor: bool,

    /// Remove runtimes started before last reboot
    #[clap(long)]
    from_before_boot: bool,
}

impl CmdRuntimePrune {
    pub async fn run(&mut self, config: &spfs::Config) -> spfs::Result<i32> {
        let runtime_storage = match &self.remote {
            Some(remote) => {
                let repo = config.get_remote(remote).await?;
                spfs::runtime::Storage::new(repo)
            }
            None => config.get_runtime_storage().await?,
        };

        // TODO: Clap 4.x AppGroup supports grouping flags better.
        if !self.from_before_boot {
            tracing::info!("No pruning strategy selected.");
            return Ok(1);
        }

        let default_author = spfs::runtime::Author::default();

        let boot_time = match procfs::Uptime::new() {
            Ok(uptime) => {
                Utc::now()
                    - Duration::from_std(uptime.uptime_duration()).map_err(|err| {
                        spfs::Error::String(format!("Failed to convert uptime duration: {err}"))
                    })?
            }
            Err(err) => {
                tracing::error!("Failed to get system uptime: {err}");
                return Ok(1);
            }
        };

        let mut runtimes = runtime_storage.iter_runtimes().await;
        while let Some(runtime) = runtimes.next().await {
            match runtime {
                Ok(runtime) => {
                    if runtime.author.created >= boot_time {
                        // This runtime is newer than the system boot time.
                        tracing::debug!(
                            created = ?runtime.author.created,
                            ?boot_time,
                            "Skipping runtime created since boot: {name}",
                            name = runtime.name()
                        );
                        continue;
                    }

                    let is_same_author = runtime.author.user_name == default_author.user_name;
                    if !self.ignore_user && !is_same_author {
                        tracing::info!(
                            "Won't delete, this runtime belongs to '{}'",
                            runtime.author.user_name
                        );
                        tracing::info!(" > use --ignore-user to ignore this error");
                        continue;
                    }

                    let is_same_host = runtime.author.host_name == default_author.host_name;
                    if !self.ignore_host && !is_same_host {
                        tracing::info!(
                            "Won't delete, this runtime was spawned on a different machine: '{}'",
                            runtime.author.host_name
                        );
                        tracing::info!(" > use --ignore-host to ignore this error");
                        continue;
                    }

                    if !self.ignore_monitor && is_same_host && is_monitor_running(&runtime) {
                        tracing::info!(
                            "Won't delete, the monitor process appears to still be running",
                        );
                        tracing::info!(
                            " > terminating the command should trigger the cleanup process"
                        );
                        tracing::info!(" > use --ignore-monitor to ignore this error");
                        continue;
                    }

                    if let Err(err) = runtime_storage.remove_runtime(runtime.name()).await {
                        tracing::error!(
                            "Failed to remove runtime {name}: {err}",
                            name = runtime.name()
                        );
                        continue;
                    }

                    tracing::info!("Pruned runtime {name}", name = runtime.name());
                }
                Err(err) => {
                    tracing::error!("Failed to read runtime: {}", err);
                }
            }
        }

        Ok(0)
    }
}

fn is_monitor_running(rt: &spfs::runtime::Runtime) -> bool {
    if let Some(pid) = rt.status.monitor {
        // we are blatantly ignoring the fact that this pid might
        // have been reused and is not the monitor anymore. Given
        // that there will always be a race condition to this effect
        // even if we did try to check the command line args for this
        // process. So we stick on the extra conservative side
        is_process_running(pid)
    } else {
        false
    }
}

fn is_process_running(pid: u32) -> bool {
    // sending a null signal to the pid just allows us to check
    // if the process actually exists without affecting it
    let pid = nix::unistd::Pid::from_raw(pid as i32);
    nix::sys::signal::kill(pid, None).is_ok()
}
