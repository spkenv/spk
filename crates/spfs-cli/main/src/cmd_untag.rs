// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use clap::Args;
use miette::Result;
use spfs::prelude::*;

/// Remove tag versions or entire tag streams
#[derive(Debug, Args)]
pub struct CmdUntag {
    /// Remove tags in a remote repository instead of the local one
    #[clap(long, short)]
    remote: Option<String>,

    /// Only remove the latest version of this tag
    #[clap(long)]
    latest: bool,

    /// Remove all versions of this tag, deleting it completely
    #[clap(short, long)]
    all: bool,

    /// The tag to remove
    ///
    /// Unless --all or --latest is provided, this must have
    /// an explicit version number (eg: path/name~0)
    #[clap(value_name = "TAG", required = true)]
    tag: String,
}

impl CmdUntag {
    pub async fn run(&mut self, config: &spfs::Config) -> Result<i32> {
        let repo = spfs::config::open_repository_from_string(config, self.remote.as_ref()).await?;

        let has_version = self.tag.contains('~') || self.latest;
        let mut tag = spfs::tracking::TagSpec::parse(&self.tag)?;
        if self.latest {
            tag = tag.with_version(0);
        }
        if !self.all && !has_version {
            tracing::error!(
                "You must specify one of --all, --latest or provide a tag with an explicit version number (eg: path/name~0)"
            );
            return Ok(1);
        }

        if self.all {
            repo.remove_tag_stream(&tag).await?;
        } else {
            let resolved = repo.resolve_tag(&tag).await?;
            repo.remove_tag(&resolved).await?;
        }
        tracing::info!(?tag, "removed");
        Ok(0)
    }
}
