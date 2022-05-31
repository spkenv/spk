// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use clap::Parser;

#[macro_use]
mod args;

main!(CmdPush);

/// Push one or more objects to a remote repository
#[derive(Debug, Parser)]
pub struct CmdPush {
    #[clap(short, long, parse(from_occurrences))]
    pub verbose: usize,

    /// The name or address of the remote server to push to
    #[clap(long, short, default_value = "origin")]
    remote: String,

    /// Forcefully sync all associated graph data even if it
    /// already exists in the destination repo
    #[clap(long)]
    no_skip_existing: bool,

    /// The reference(s) to push
    #[clap(value_name = "REF", required = true)]
    refs: Vec<String>,
}

impl CmdPush {
    pub async fn run(&mut self, config: &spfs::Config) -> spfs::Result<i32> {
        let repo = config.get_local_repository().await?.into();
        let remote = config.get_remote(&self.remote).await?;

        let mut summary = spfs::sync::SyncSummary::default();
        for reference in self.refs.iter() {
            summary += spfs::Syncer::new(&repo, &remote)
                .set_skip_existing_objects(!self.no_skip_existing)
                .set_skip_existing_payloads(!self.no_skip_existing)
                .sync_ref(reference)
                .await?
                .summary();
        }
        tracing::info!("{}", spfs::io::format_sync_summary(&summary));

        Ok(0)
    }
}
