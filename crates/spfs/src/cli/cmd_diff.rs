// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use clap::Args;

/// Compare two spfs file system states
#[derive(Debug, Args)]
pub struct CmdDiff {
    /// The tag or id to use as the base of the computed diff, defaults to the current runtime
    #[clap(value_name = "FROM")]
    base: Option<String>,

    /// The tag or id to diff the base against, defaults to the contents of the spfs filesystem
    #[clap(value_name = "TO")]
    top: Option<String>,
}

impl CmdDiff {
    pub async fn run(&mut self, _config: &spfs::Config) -> spfs::Result<i32> {
        let diffs = spfs::diff(self.base.as_ref(), self.top.as_ref()).await?;
        let out = spfs::io::format_changes(diffs.iter());
        if out.trim().is_empty() {
            tracing::info!("no changes");
        } else {
            println!("{out}");
        }
        Ok(0)
    }
}
