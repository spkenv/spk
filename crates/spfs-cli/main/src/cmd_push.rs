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

    /// The source repository to push from
    ///
    /// Can be a remote name or address. Defaults to the local repository.
    #[clap(long)]
    from: Option<String>,

    /// The destination repository to push to
    ///
    /// Can be a remote name or address. Defaults to "origin".
    /// Cannot be used together with --remote.
    #[clap(long, conflicts_with = "remote")]
    to: Option<String>,

    /// The reference(s) to push
    ///
    /// These can be individual tags or digests, or they may also
    /// be a collection of items joined by a '+'
    #[clap(value_name = "REF", required = true)]
    refs: Vec<spfs::tracking::EnvSpec>,
}

impl CmdPush {
    pub async fn run(&mut self, config: &spfs::Config) -> Result<i32> {
        // --remote is an alias for --to
        let to = self
            .to
            .take()
            .or_else(|| self.repos.remote.take())
            .or_else(|| Some("origin".to_string()));

        let src = spfs::config::open_repository_from_string(config, self.from.as_ref());
        let dest = spfs::config::open_repository_from_string(config, to.as_ref());
        let (src, dest) = tokio::try_join!(src, dest)?;

        let env_spec = self.refs.iter().cloned().collect();
        // the latest tag is always synced when pushing
        self.sync.sync = true;
        let summary = self
            .sync
            .get_syncer(&src, &dest)
            .sync_env(env_spec)
            .await?
            .summary();
        tracing::info!("{}", spfs::io::format_sync_summary(&summary));

        Ok(0)
    }
}
