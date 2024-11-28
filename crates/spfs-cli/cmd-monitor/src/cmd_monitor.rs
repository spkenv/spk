// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::time::Duration;

use clap::Parser;
#[cfg(feature = "sentry")]
use cli::configure_sentry;
use miette::{Context, IntoDiagnostic, Result};
use spfs_cli_common as cli;
use spfs_cli_common::CommandName;
use tokio::io::AsyncReadExt;
use tokio::time::timeout;

mod signal;
#[cfg(unix)]
use signal::unix_signal_handler::UnixSignalHandler as SignalHandlerImpl;
use signal::SignalHandler;
#[cfg(windows)]
use windows_signal_handler::WindowsSignalHandler as SignalHandlerImpl;

fn main() -> Result<()> {
    // because this function exits right away it does not
    // properly handle destruction of data, so we put the actual
    // logic into a separate function/scope
    std::process::exit(main2()?);
}
fn main2() -> Result<i32> {
    let mut opt = CmdMonitor::parse();
    opt.logging
        .log_file
        .get_or_insert("/tmp/spfs-runtime/monitor.log".into());

    // This disables sentry (the first boolean literal), and enables
    // syslog (the second literal). The sentry initialization is
    // managed directly in this command due to how it daemonizes
    // itself.
    let (config, _empty_sentry_guard) = cli::configure!(opt, false, true);

    let result = opt.run(&config);

    spfs_cli_common::handle_result!(result)
}

/// Takes ownership of, and is responsible for monitoring an active runtime.
///
/// There should be exactly one monitor for each runtime process and the monitor
/// will clean up the runtime when all processes exit.
#[derive(Debug, Parser)]
pub struct CmdMonitor {
    #[clap(flatten)]
    pub logging: cli::Logging,

    /// Do not change the current working directory to / when daemonizing
    #[clap(long, env = "SPFS_MONITOR_NO_CHDIR")]
    no_chdir: bool,

    /// Do not close stdin, stdout, and stderr when daemonizing
    #[clap(long, env = "SPFS_MONITOR_NO_CLOSE")]
    no_close: bool,

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
    pub fn run(&mut self, config: &spfs::Config) -> Result<i32> {
        // create an initial runtime that will wait for the
        // caller to signal that we are ready to start processing
        let rt = tokio::runtime::Builder::new_current_thread()
            // this runtime only ever needs to wait for one io stream
            // from the parent, and will then be shutdown in favor
            // of a new runtime created after daemonization
            .max_blocking_threads(1)
            .enable_all()
            .build()
            .into_diagnostic()
            .wrap_err("Failed to establish async runtime")?;
        rt.block_on(self.wait_for_ready());
        // clean up this runtime and all other threads before detaching
        drop(rt);
        #[cfg(unix)]
        nix::unistd::daemon(self.no_chdir, self.no_close)
            .into_diagnostic()
            .wrap_err("Failed to daemonize the monitor process")?;
        #[cfg(feature = "sentry")]
        {
            // Initialize sentry after the call to `daemon` so it is safe for
            // sentry to create threads and mutexes. The result can be dropped
            // here because the sentry guard lives on inside a lazy init
            // static.
            let _ = configure_sentry(self.command_name().to_owned());
        }

        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(config.monitor.worker_threads.get())
            .max_blocking_threads(config.monitor.max_blocking_threads.get())
            .enable_all()
            .build()
            .into_diagnostic()
            .wrap_err("Failed to establish async runtime")?;
        let code = rt.block_on(self.run_async(config))?;
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

    pub async fn run_async(&mut self, config: &spfs::Config) -> Result<i32> {
        let signal_future = SignalHandlerImpl::build_signal_future();

        let repo = spfs::open_repository(&self.runtime_storage).await?;
        let storage = spfs::runtime::Storage::new(repo)?;
        let runtime = storage.read_runtime(&self.runtime).await?;
        tracing::trace!("read runtime from storage repo");

        let mut owned = spfs::runtime::OwnedRuntime::upgrade_as_monitor(runtime).await?;
        tracing::trace!("upgraded to owned runtime, waiting for empty runtime");

        let fut = spfs::monitor::wait_for_empty_runtime(&owned, config);
        let res = tokio::select! {
            res = fut => {
                tracing::info!("Monitor detected no more processes, cleaning up runtime...");
                res
            }
            // we explicitly catch any signal related to interruption
            // and will act by cleaning up the runtime early
            _ = signal_future => Err(spfs::Error::String("Signal received, cleaning up runtime early".to_string())),
        };
        tracing::trace!("runtime empty of processes ");

        // need to reload the runtime here to get any changes made to
        // the runtime while it was running so we don't blast them the
        // next time this process saves the runtime state.
        tracing::trace!("reloading runtime data before cleanup");
        owned.reload_state_from_storage().await?;

        // try to set the running to false to make this
        // runtime easier to identify as safe to delete
        // if the automatic cleanup fails. Any error
        // here is unfortunate but not fatal.
        owned.status.running = false;
        if let Err(err) = owned.save_state_to_storage().await {
            tracing::error!("failed to save runtime: {err:?}");
        }

        tracing::trace!("tearing down and exiting");
        if let Err(err) = spfs::exit_runtime(&owned).await {
            tracing::error!("failed to tear down runtime: {err:?}");
        }

        tracing::trace!(
            "{} runtime data",
            if owned.is_durable() {
                "keeping"
            } else {
                "deleting"
            }
        );
        if owned.is_durable() {
            // Reset the runtime can be rerun in future.  This has to
            // be done after the exit_runtime/spfs enter --exit is
            // called because that command relies on the
            // mount_namespace value, which this resets, to teardown
            // the runtime.
            if let Err(err) = owned.reinit_for_reuse_and_save_to_storage().await {
                tracing::error!("failed to reset durable runtime for rerunning: {err:?}")
            }
        } else if let Err(err) = owned.delete().await {
            tracing::error!("failed to clean up runtime data: {err:?}")
        }

        res?;
        Ok(0)
    }
}
