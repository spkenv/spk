// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use clap::Args;
use miette::Result;
use spfs::prelude::*;
use spfs::{self};
use spfs_cli_common as cli;

/// Tag an object
#[derive(Debug, Args)]
pub struct CmdTag {
    #[clap(flatten)]
    pub(crate) repos: cli::Repositories,

    /// The reference or id of the item to tag
    #[clap(value_name = "TARGET_REF")]
    reference: String,

    /// The tag(s) to point to the the given target
    #[clap(value_name = "TAG", required = true)]
    tags: Vec<String>,
}

impl CmdTag {
    pub async fn run(&mut self, config: &spfs::Config) -> Result<i32> {
        let repo =
            spfs::config::open_repository_from_string(config, self.repos.remote.as_ref()).await?;

        let target = repo.read_ref(self.reference.as_str()).await?.digest()?;
        for tag in self.tags.iter() {
            let tag = tag.parse()?;
            repo.push_tag(&tag, &target).await?;
            tracing::info!(?tag, "created");
        }
        Ok(0)
    }
}
