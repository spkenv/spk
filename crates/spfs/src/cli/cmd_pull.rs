// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use clap::Parser;

#[macro_use]
mod args;

main!(CmdPull);

/// Pull one or more objects to the local repository
#[derive(Debug, Parser)]
pub struct CmdPull {
    #[clap(short, long, parse(from_occurrences))]
    pub verbose: usize,

    /// The name or address of the remote server to pull from
    ///
    /// Defaults to searching all configured remotes
    #[clap(long, short)]
    remote: Option<String>,

    /// Forcefully sync all associated graph data even if it
    /// already exists in the local repo
    #[clap(long)]
    no_skip_existing: bool,

    /// The reference(s) to pull/localize
    #[clap(value_name = "REF", required = true)]
    refs: Vec<String>,
}

impl CmdPull {
    pub async fn run(&mut self, config: &spfs::Config) -> spfs::Result<i32> {
        let repo = config.get_local_repository_handle().await?;
        let remote = match &self.remote {
            None => config.get_remote("origin").await?,
            Some(remote) => config.get_remote(remote).await?,
        };

        let mut summary = spfs::sync::SyncSummary::default();
        for reference in self.refs.iter() {
            summary += spfs::Syncer::new(&remote, &repo)
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
