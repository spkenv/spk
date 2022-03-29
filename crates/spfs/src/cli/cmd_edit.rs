// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use clap::Args;

/// Make the current runtime editable
#[derive(Debug, Args)]
pub struct CmdEdit {
    /// Disable edit mode instead
    #[clap(long)]
    off: bool,
}

impl CmdEdit {
    pub async fn run(&mut self, _config: &spfs::Config) -> spfs::Result<i32> {
        if !self.off {
            spfs::make_active_runtime_editable().await?;
            tracing::info!("edit mode enabled");
        } else {
            let mut rt = spfs::active_runtime()?;
            rt.set_editable(false)?;
            if let Err(err) = spfs::remount_runtime(&rt).await {
                rt.set_editable(true)?;
                return Err(err);
            }
            tracing::info!("edit mode disabled");
        }
        Ok(0)
    }
}
