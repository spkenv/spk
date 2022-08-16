// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use clap::Args;

use spfs::{self, prelude::*};

/// Tag an object
#[derive(Debug, Args)]
pub struct CmdTag {
    /// Create tags in a remote repository instead of the local one
    #[clap(long, short)]
    remote: Option<String>,

    /// The reference or id of the item to tag
    #[clap(value_name = "TARGET_REF")]
    reference: String,

    /// The tag(s) to point to the the given target
    #[clap(value_name = "TAG", required = true)]
    tags: Vec<String>,
}

impl CmdTag {
    pub async fn run(&mut self, config: &spfs::Config) -> spfs::Result<i32> {
        let repo = spfs::config::open_repository_from_string(config, self.remote.as_ref()).await?;

        let target = repo.read_ref(self.reference.as_str()).await?.digest()?;
        for tag in self.tags.iter() {
            let tag = tag.parse()?;
            repo.push_tag(&tag, &target).await?;
            tracing::info!(?tag, "created");
        }
        Ok(0)
    }
}
