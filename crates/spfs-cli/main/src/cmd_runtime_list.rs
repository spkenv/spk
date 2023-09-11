// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use anyhow::Result;
use clap::Args;
use futures::StreamExt;
use spfs::monitor::find_processes_and_mount_namespaces;

/// List runtime information from the repository
#[derive(Debug, Args)]
#[clap(visible_alias = "ls")]
pub struct CmdRuntimeList {
    /// List runtimes in a remote or alternate repository
    #[clap(short, long)]
    remote: Option<String>,

    /// Only print the name of each runtime, no additional data
    #[clap(short, long)]
    quiet: bool,
}

impl CmdRuntimeList {
    pub async fn run(&mut self, config: &spfs::Config) -> Result<i32> {
        let runtime_storage = match &self.remote {
            Some(remote) => {
                let repo = config.get_remote(remote).await?;
                spfs::runtime::Storage::new(repo)
            }
            None => config.get_runtime_storage().await?,
        };

        let known_processes = find_processes_and_mount_namespaces().await?;

        let mut runtimes = runtime_storage.iter_runtimes().await;
        while let Some(runtime) = runtimes.next().await {
            match runtime {
                Ok(runtime) => {
                    let mut message = runtime.name().to_string();
                    if !self.quiet {
                        let owner_running = runtime
                            .status
                            .owner
                            .map(|pid| known_processes.contains_key(&pid));

                        let monitor_running = runtime
                            .status
                            .monitor
                            .map(|pid| known_processes.contains_key(&pid));

                        let processes_exist_with_mount_namespace =
                            runtime.config.mount_namespace.as_ref().map(|runtime_ns| {
                                known_processes.values().any(
                                    |process_ns| matches!(process_ns, Some(ns) if ns == runtime_ns),
                                )
                            });

                        // Pick a word to describe the status of the runtime,
                        // in terms of if any processes or the monitor have
                        // been found to still exist.
                        //
                        // These words are designed to be distinct from each
                        // other for use with grep.
                        let process_status = match (
                            owner_running,
                            monitor_running,
                            processes_exist_with_mount_namespace,
                        ) {
                            (Some(true), Some(false), _) | (_, Some(false), Some(true)) => {
                                // The monitor has died while processes still
                                // exist.
                                "unmonitored"
                            }
                            (Some(true), _, _)
                            | (_, _, Some(true))
                            | (Some(false), Some(true), None) => {
                                // Either know for sure some processes are
                                // still alive, or assume because the monitor
                                // is still running.
                                "running"
                            }
                            (Some(false), Some(true), Some(false)) => {
                                // This could be a case of a zombie
                                // spfs-monitor that will never quit on its
                                // own.
                                "stopping"
                            }
                            (Some(false), _, Some(false)) => "stopped",
                            (Some(false), Some(false), None) => {
                                // This case the namespace is unknown, which
                                // will be uncommon. Assume that because the
                                // monitor stopped all the processes are gone.
                                "stopped"
                            }
                            (Some(false), None, None) => {
                                // The owner is gone and the monitor/namespace
                                // is unknown. This is probably a stale
                                // runtime.
                                "zombie"
                            }
                            (None, None, _) => {
                                // There's no owner or monitor but the
                                // durable runtime remains
                                if runtime.config.durable {
                                    "saved"
                                } else {
                                    "unknown"
                                }
                            }
                            (None, _, _) => {
                                // these cases aren't expected
                                "unknown"
                            }
                        };

                        message = format!(
                            "{message:37}\trunning={}\tpid={:<7}\teditable={}\tdurable={}\tstatus={process_status}",
                            runtime.status.running,
                            runtime
                                .status
                                .owner
                                .map(|pid| pid.to_string())
                                .unwrap_or_else(|| "unknown".to_string()),
                            runtime.status.editable,
                            runtime.keep_runtime(),
                        )
                    }
                    println!("{message}");
                }
                Err(err) if !self.quiet => {
                    eprintln!("Failed to read runtime: {err}");
                }
                Err(_) => {}
            }
        }
        Ok(0)
    }
}
