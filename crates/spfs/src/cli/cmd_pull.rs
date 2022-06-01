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
    #[clap(flatten)]
    sync: args::Sync,

    #[clap(short, long, parse(from_occurrences))]
    verbose: usize,

    /// The name or address of the remote server to pull from
    ///
    /// Defaults to searching all configured remotes
    #[clap(long, short)]
    remote: Option<String>,

    /// The reference(s) to pull/localize
    ///
    /// These can be individual tags or digests, or they may also
    /// be a collection of items joined by a '+'
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

        let env_spec =
            spfs::tracking::EnvSpec::parse(self.refs.join(spfs::tracking::ENV_SPEC_SEPARATOR))?;
        let summary = self
            .sync
            .get_syncer(&remote, &repo)
            .sync_env(env_spec)
            .await?
            .summary();

        tracing::info!("{}", spfs::io::format_sync_summary(&summary));

        Ok(0)
    }
}
