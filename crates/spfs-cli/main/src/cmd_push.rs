// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use clap::Args;
use miette::Result;
use spfs_cli_common as cli;

/// Push one or more objects to a remote repository
#[derive(Debug, Args)]
pub struct CmdPush {
    #[clap(flatten)]
    sync: cli::Sync,

    #[clap(short, long, action = clap::ArgAction::Count)]
    verbose: u8,

    #[clap(flatten)]
    pub(crate) repos: cli::Repositories,

    /// The reference(s) to push
    ///
    /// These can be individual tags or digests, or they may also
    /// be a collection of items joined by a '+'
    #[clap(value_name = "REF", required = true)]
    refs: Vec<spfs::tracking::EnvSpec>,
}

impl CmdPush {
    pub async fn run(&mut self, config: &spfs::Config) -> Result<i32> {
        let (repo, remote) = tokio::try_join!(
            config.get_local_repository_handle(),
            spfs::config::open_repository_from_string(config, self.repos.remote.as_ref()),
        )?;

        let env_spec = self.refs.iter().cloned().collect();
        // the latest tag is always synced when pushing
        self.sync.sync = true;
        let summary = self
            .sync
            .get_syncer(&repo, &remote)
            .sync_env(env_spec)
            .await?
            .summary();
        tracing::info!("{}", spfs::io::format_sync_summary(&summary));

        Ok(0)
    }
}
