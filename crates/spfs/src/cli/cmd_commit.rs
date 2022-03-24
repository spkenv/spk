// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::path::PathBuf;

use spfs::prelude::*;
use structopt::StructOpt;

use spfs::{encoding::Encodable, storage::TagStorage};

#[derive(Debug, StructOpt)]
pub struct CmdCommit {
    #[structopt(
        long = "remote",
        short = "r",
        about = "commit files directly into a remote repository instead of the local one, works only with the --path arg"
    )]
    remote: Option<String>,
    #[structopt(
        long = "tag",
        short = "t",
        about = "Can be given many times: human-readable tags to update with the resulting object"
    )]
    tags: Vec<String>,
    #[structopt(
        long,
        about = "Commit this directory as a layer rather than the current spfs changes"
    )]
    path: Option<PathBuf>,
    #[structopt(
        possible_values = &["layer", "platform"],
        conflicts_with_all = &["path", "remote"],
        required_unless = "path",
        about = "The desired object type to create, skip this when giving --path"
    )]
    kind: Option<String>,
}

impl CmdCommit {
    pub async fn run(&mut self, config: &spfs::Config) -> spfs::Result<i32> {
        let repo = match &self.remote {
            Some(remote) => config.get_remote(remote).await?,
            None => config.get_repository().await?.into(),
        };

        let result: spfs::graph::Object = if let Some(path) = &self.path {
            let manifest = repo.commit_dir(&path).await?;
            if manifest.is_empty() {
                return Err(spfs::Error::NothingToCommit);
            }
            repo.create_layer(&spfs::graph::Manifest::from(&manifest))
                .await?
                .into()
        } else {
            // no path give, commit the current runtime

            let mut runtime = spfs::active_runtime()?;

            if !runtime.is_editable() {
                tracing::error!("Active runtime is not editable, nothing to commmit");
                return Ok(1);
            }

            match self.kind.clone().unwrap_or_default().as_str() {
                "layer" => spfs::commit_layer(&mut runtime).await?.into(),
                "platform" => spfs::commit_platform(&mut runtime).await?.into(),
                kind => {
                    tracing::error!("don't know how to commit a '{}'", kind);
                    return Ok(1);
                }
            }
        };

        tracing::info!(digest = ?result.digest()?, "created");
        for tag in self.tags.iter() {
            let tag_spec = match spfs::tracking::TagSpec::parse(tag) {
                Ok(tag_spec) => tag_spec,
                Err(err) => {
                    tracing::warn!("cannot set invalid tag '{tag}': {err:?}");
                    continue;
                }
            };
            repo.push_tag(&tag_spec, &result.digest()?).await?;
            tracing::info!(?tag, "created");
        }
        if self.kind.is_some() {
            tracing::info!("edit mode disabled");
        }

        Ok(0)
    }
}
