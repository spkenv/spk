// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

#![deny(unsafe_op_in_unsafe_fn)]
#![warn(clippy::fn_params_excessive_bools)]

use std::time::Duration;

use anyhow::{Context, Result};
use clap::Parser;
use spfs::Error;
use spfs_cli_common as cli;
use spfs_cli_common::CommandName;
use tokio::io::AsyncReadExt;
use tokio::signal::unix::{signal, SignalKind};
use tokio::time::timeout;

cli::main!(CmdMonitor, sentry = true, sync = true, syslog = true);

/// Takes ownership of, and is responsible for monitoring an active runtime.
///
/// There should be exactly one monitor for each runtime process and the monitor
/// will clean up the runtime when all processes exit.
#[derive(Debug, Parser)]
pub struct CmdMonitor {
    #[clap(flatten)]
    pub logging: cli::Logging,

    /// The address of the storage being used for runtimes
    #[clap(long)]
    runtime_storage: url::Url,

    /// The name of the runtime being monitored
    #[clap(long)]
    runtime: String,
}

impl CommandName for CmdMonitor {
    fn command_name(&self) -> &'static str {
        "monitor"
    }
}

impl CmdMonitor {
    pub fn run(&mut self, _config: &spfs::Config) -> Result<i32> {
        // create an initial runtime that will wait for the
        // caller to signal that we are ready to start processing
        let rt = tokio::runtime::Builder::new_current_thread()
            .max_blocking_threads(1)
            .enable_all()
            .build()
            .context("Failed to establish async runtime")?;
        rt.block_on(self.wait_for_ready());
        // clean up this runtime and all other threads before detaching
        drop(rt);

        const NO_CHDIR: bool = false;
        const NO_CLOSE: bool = false;
        nix::unistd::daemon(NO_CHDIR, NO_CLOSE)
            .context("Failed to daemonize the monitor process")?;

        let rt = tokio::runtime::Builder::new_multi_thread()
            .max_blocking_threads(2)
            .enable_all()
            .build()
            .context("Failed to establish async runtime")?;
        let code = rt.block_on(self.run_async())?;
        // the monitor is running in the background and, although not expected,
        // can take extra time to shutdown if needed
        rt.shutdown_timeout(std::time::Duration::from_secs(5));
        Ok(code)
    }

    pub async fn wait_for_ready(&self) {
        // Wait to be informed that it is now safe to read the mount namespace
        // of the pid we are meant to be monitoring. Also wait to read/write
        // the runtime as well. spfs-enter promises to not modify the runtime
        // once it sends us a message.
        let mut stdin = tokio::io::stdin();
        let mut buffer = [0; 64];
        let wait_for_ok = stdin.read(&mut buffer);
        // Some large environments can take many minutes to render. Be generous
        // with this timeout.
        match timeout(Duration::from_secs(3600), wait_for_ok).await {
            Ok(Ok(bytes_read)) if bytes_read > 0 => {
                tracing::debug!(
                    ?bytes_read,
                    "Read from parent process: {:?}",
                    &buffer[..bytes_read]
                );
            }
            Ok(_) => {
                tracing::warn!("Parent process quit before sending us anything!");
            }
            Err(_) => {
                tracing::warn!("Timeout waiting for parent process!");
            }
        }
    }

    pub async fn run_async(&mut self) -> Result<i32> {
        let mut interrupt = signal(SignalKind::interrupt())
            .map_err(|err| Error::process_spawn_error("signal()".into(), err, None))?;
        let mut quit = signal(SignalKind::quit())
            .map_err(|err| Error::process_spawn_error("signal()".into(), err, None))?;
        let mut terminate = signal(SignalKind::terminate())
            .map_err(|err| Error::process_spawn_error("signal()".into(), err, None))?;

        let repo = spfs::open_repository(&self.runtime_storage).await?;
        let storage = spfs::runtime::Storage::new(repo);
        let runtime = storage.read_runtime(&self.runtime).await?;

        let mut owned = spfs::runtime::OwnedRuntime::upgrade_as_monitor(runtime).await?;

        let fut = spfs::env::wait_for_empty_runtime(&owned);
        let res = tokio::select! {
            res = fut => {
                tracing::info!("Monitor detected no more processes, cleaning up runtime...");
                res
            }
            // we explicitly catch any signal related to interruption
            // and will act by cleaning up the runtime early
            _ = terminate.recv() => Err(spfs::Error::String("Terminate signal received, cleaning up runtime early".to_string())),
            _ = interrupt.recv() => Err(spfs::Error::String("Interrupt signal received, cleaning up runtime early".to_string())),
            _ = quit.recv() => Err(spfs::Error::String("Quit signal received, cleaning up runtime early".to_string())),
        };

        // try to set the running to false to make this
        // runtime easier to identify as safe to delete
        // if the automatic cleanup fails. Any error
        // here is unfortunate but not fatal.
        owned.status.running = false;
        let _ = owned.save_state_to_storage().await;

        match owned.config.mount_backend {
            spfs::runtime::MountBackend::OverlayFsWithRenders => {}
            spfs::runtime::MountBackend::OverlayFsWithFuse
            | spfs::runtime::MountBackend::FuseOnly => {
                // the mounted FUSE filesystem needs to be explicitly unmounted
                // upon exit as the daemonized server will keep the mount namespace
                // alive and never exit
                const LAZY: bool = false; // it must clean up fully because the runtime is shutting down
                if let Err(err) = spfs::env::unmount_env_fuse(&owned, LAZY).await {
                    tracing::error!("{err}");
                }
            }
        }
        if let Err(err) = owned.delete().await {
            tracing::error!("failed to clean up runtime data: {err:?}")
        }

        res?;
        Ok(0)
    }
}
