// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use clap::Args;
use tokio::signal::unix::{signal, SignalKind};

/// Takes ownership of, and is responsible for monitoring an active runtime.
///
/// There should be exactly one monitor for each runtime process and the monitor
/// will clean up the runtime when all processes exit.
#[derive(Debug, Args)]
pub struct CmdMonitor {
    /// The address of the storage being used for runtimes
    #[clap(long)]
    runtime_storage: url::Url,

    /// The name of the runtime being entered
    #[clap(long)]
    runtime: String,
}

impl CmdMonitor {
    pub async fn run(&mut self, _config: &spfs::Config) -> spfs::Result<i32> {
        let mut interrupt = signal(SignalKind::interrupt())?;
        let mut quit = signal(SignalKind::quit())?;
        let repo = spfs::open_repository(&self.runtime_storage).await?;
        let storage = spfs::runtime::Storage::new(repo);
        let runtime = storage.read_runtime(&self.runtime).await?;

        let mut owned = spfs::runtime::OwnedRuntime::upgrade_as_monitor(runtime).await?;
        let fut = spfs::env::wait_for_empty_runtime(&owned);
        let res = tokio::select! {
            res = fut => res,
            // we explicitly catch any signal related to interruption
            // and will act by cleaning up the runtime early
            _ = interrupt.recv() => Err(spfs::Error::String("Interrupt signal received, cleaning up runtime early".to_string())),
            _ = quit.recv() => Err(spfs::Error::String("Quit signal received, cleaning up runtime early".to_string())),
        };

        // try to set the running to false to make this
        // runtime easier to identify as safe to delete
        // if the automatic cleanup fails. Any error
        // here is unfortunate but not fatal.
        owned.status.running = false;
        let _ = owned.save().await;
        if let Err(err) = owned.delete().await {
            tracing::error!("failed to clean up runtime data: {err:?}")
        }
        res?;
        Ok(0)
    }
}
