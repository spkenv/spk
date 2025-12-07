// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use chrono::{Duration, Utc};
use clap::Args;
use miette::Result;
#[cfg(target_os = "linux")]
use procfs::Current;
use spfs_cli_common as cli;
use tokio_stream::StreamExt;

use super::cmd_runtime_remove::is_monitor_running;

/// Find and remove runtimes from the repository based on a pruning strategy
#[derive(Debug, Args)]
pub struct CmdRuntimePrune {
    #[clap(flatten)]
    pub(crate) repos: cli::Repositories,

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

    /// Ignore runtimes that appear to be zombies
    #[clap(long)]
    ignore_zombies: bool,

    /// Remove runtimes started before last reboot
    #[clap(long)]
    from_before_boot: bool,
}

impl CmdRuntimePrune {
    pub async fn run(&mut self, config: &spfs::Config) -> Result<i32> {
        let runtime_storage = match &self.repos.remote {
            Some(remote) => {
                let repo = config.get_remote(remote).await?;
                spfs::runtime::Storage::new(repo)?
            }
            None => config.get_runtime_storage().await?,
        };

        // TODO: Clap 4.x AppGroup supports grouping flags better.
        if !self.from_before_boot && self.ignore_zombies {
            tracing::info!("No pruning strategy selected.");
            if self.ignore_zombies {
                tracing::info!(" > remove --ignore-zombies to enable pruning of zombie runtimes");
            }
            if !self.from_before_boot {
                tracing::info!(
                    " > add --from-before-boot to enable pruning of runtimes created before last reboot"
                );
            }
            return Ok(1);
        }

        let default_author = spfs::runtime::Author::default();

        #[cfg(target_os = "linux")]
        let boot_time = match procfs::Uptime::current() {
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
        #[cfg(target_os = "macos")]
        let boot_time = {
            // On macOS, use sysctl to get boot time
            use std::process::Command;
            let output = Command::new("sysctl")
                .args(["-n", "kern.boottime"])
                .output()
                .map_err(|err| spfs::Error::String(format!("Failed to get boot time: {err}")))?;
            let stdout = String::from_utf8_lossy(&output.stdout);
            // Parse the format: "{ sec = 1234567890, usec = 123456 } ..."
            let sec_str = stdout
                .split("sec = ")
                .nth(1)
                .and_then(|s| s.split(',').next())
                .ok_or_else(|| spfs::Error::String("Failed to parse boot time".to_string()))?;
            let sec: i64 = sec_str.trim().parse().map_err(|err| {
                spfs::Error::String(format!("Failed to parse boot time seconds: {err}"))
            })?;
            chrono::DateTime::from_timestamp(sec, 0)
                .ok_or_else(|| spfs::Error::String("Failed to convert boot time".to_string()))?
        };
        #[cfg(windows)]
        let boot_time = Utc::now()
            - Duration::milliseconds(unsafe {
                // Safety: this is a raw system API, but seems infallible nonetheless
                windows::Win32::System::SystemInformation::GetTickCount64() as i64
            });

        let mut runtimes = runtime_storage.iter_runtimes().await;
        while let Some(runtime) = runtimes.next().await {
            match runtime {
                Ok(runtime) => {
                    if runtime.author.created >= boot_time && self.from_before_boot {
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

                    if runtime.is_durable() {
                        // Durable runtimes are not considered trash
                        // runtimes, they are not suitable for pruning.
                        tracing::info!(
                            "Won't delete {}, the runtime is durable. Use `spk runtime rm` to delete it.",
                            runtime.name()
                        );
                        continue;
                    }

                    if self.ignore_zombies && runtime.is_zombie() {
                        tracing::info!(
                            "Won't delete zombie runtime {}, ignore-zombies is set.",
                            runtime.name()
                        );
                        tracing::info!(" > remove --ignore-zombies to ignore this error");
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
