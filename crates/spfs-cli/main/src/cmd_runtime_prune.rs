// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use anyhow::Result;
use chrono::{Duration, Utc};
use clap::Args;
use tokio_stream::StreamExt;

use super::cmd_runtime_remove::is_monitor_running;

/// Find and remove runtimes from the repository based on a pruning strategy
#[derive(Debug, Args)]
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

    /// Allow durable runtimes to be removed, normally they will not
    /// be removed by pruning
    #[clap(long)]
    remove_durable: bool,

    /// Remove runtimes started before last reboot
    #[clap(long)]
    from_before_boot: bool,
}

impl CmdRuntimePrune {
    pub async fn run(&mut self, config: &spfs::Config) -> Result<i32> {
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

        #[cfg(unix)]
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
        #[cfg(windows)]
        let boot_time = Utc::now()
            - Duration::milliseconds(unsafe {
                // Safety: this is a raw system API, but seems infallible nontheless
                windows::Win32::System::SystemInformation::GetTickCount64() as i64
            });

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

                    if runtime.keep_runtime() && !self.remove_durable {
                        tracing::info!("Won't delete, the runtime is marked as durable and");
                        tracing::info!(" > '--remove-durable' was not specified");
                        tracing::info!(" > use --remove-durable to remove durable runtimes");
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
