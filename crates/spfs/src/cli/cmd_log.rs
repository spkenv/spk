// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use clap::Args;
use colored::*;
use futures::StreamExt;

use spfs::{self};

/// Log the history of a given tag over time
#[derive(Debug, Args)]
pub struct CmdLog {
    /// Load the tag from remote repository instead of the local one
    #[clap(long, short)]
    remote: Option<String>,

    /// The tag to show history of
    tag: String,
}

impl CmdLog {
    pub async fn run(&mut self, config: &spfs::Config) -> spfs::Result<i32> {
        let repo = spfs::config::open_repository_from_string(config, self.remote.as_ref()).await?;

        let tag = spfs::tracking::TagSpec::parse(&self.tag)?;
        let mut tag_stream = repo.read_tag(&tag).await?.enumerate();
        while let Some((i, tag)) = tag_stream.next().await {
            let tag = tag?;
            let spec = spfs::tracking::build_tag_spec(tag.org(), tag.name(), i as u64)?;
            let spec_str = spec.to_string();
            println!(
                "{} {} {} {}",
                tag.target.to_string()[..10].yellow(),
                spec_str.bold(),
                tag.user.bright_blue(),
                tag.time.to_string().green(),
            );
        }
        Ok(0)
    }
}
