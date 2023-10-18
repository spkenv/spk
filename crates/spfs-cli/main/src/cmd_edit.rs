// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use anyhow::Result;
use clap::Args;

/// Make the current runtime editable
#[derive(Debug, Args)]
pub struct CmdEdit {
    /// Disable edit mode instead
    #[clap(long)]
    off: bool,

    /// Change a runtime into a durable runtime, will also make the runtime editable
    #[clap(long)]
    keep_runtime: bool,
}

impl CmdEdit {
    pub async fn run(&mut self, _config: &spfs::Config) -> Result<i32> {
        // Making a durable runtime is processed first, because the
        // runtime may already be editable and checking that first
        // will cause an error in make_active_runtime_editable() below.
        if self.keep_runtime {
            tracing::debug!("trying to keep runtime");
            let rt = spfs::active_runtime().await?;
            if rt.is_durable() {
                tracing::info!("runtime is already durable");
            } else {
                tracing::debug!("making runtime durable");
                spfs::make_runtime_durable(&rt).await?;
                tracing::info!("runtime changed to durable");
            }
        }

        if !self.off {
            match spfs::make_active_runtime_editable().await {
                Ok(_) => tracing::info!("edit mode enabled"),
                Err(err) => {
                    if self.keep_runtime {
                        // When --keep-runtime was also given, being
                        // already editable isn't an error, but still
                        // want to tell the user about it
                        tracing::info!("{err}")
                    } else {
                        return Err(err.into());
                    }
                }
            };
        } else {
            let mut rt = spfs::active_runtime().await?;
            rt.status.editable = false;
            rt.save_state_to_storage().await?;
            spfs::remount_runtime(&rt).await?;
            tracing::info!("edit mode disabled");
        }

        Ok(0)
    }
}
