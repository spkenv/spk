// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::path::PathBuf;
use std::sync::Arc;

use clap::Args;
use spfs::encoding::Encodable;

/// Commit the current runtime state or a directory to storage
#[derive(Debug, Args)]
pub struct CmdCommit {
    /// Commit files directly into a remote repository
    ///
    /// The default is to commit to the local repository. This flag
    /// only with the --path argument
    #[clap(long, short)]
    remote: Option<String>,

    /// A human-readable tag for the generated object
    ///
    /// Can be provided more than once.
    #[clap(long = "tag", short)]
    tags: Vec<String>,

    /// Commit this directory instead of the current spfs changes
    #[clap(long)]
    path: Option<PathBuf>,

    /// The desired object type to create, skip this when giving --path
    #[clap(
        possible_values = &["layer", "platform"],
        conflicts_with_all = &["path", "remote"],
        required_unless_present = "path",
    )]
    kind: Option<String>,
}

impl CmdCommit {
    pub async fn run(&mut self, config: &spfs::Config) -> spfs::Result<i32> {
        let repo = Arc::new(
            config
                .get_remote_repository_or_local(self.remote.as_ref())
                .await?,
        );

        let result: spfs::graph::Object = if let Some(path) = &self.path {
            let manifest = spfs::commit_dir(Arc::clone(&repo), path).await?;
            if manifest.is_empty() {
                return Err(spfs::Error::NothingToCommit);
            }
            repo.create_layer(&spfs::graph::Manifest::from(&manifest))
                .await?
                .into()
        } else {
            // no path give, commit the current runtime

            let mut runtime = spfs::active_runtime().await?;

            if !runtime.status.editable {
                tracing::error!("Active runtime is not editable, nothing to commit");
                return Ok(1);
            }

            match self.kind.clone().unwrap_or_default().as_str() {
                "layer" => spfs::commit_layer(&mut runtime, Arc::clone(&repo))
                    .await?
                    .into(),
                "platform" => spfs::commit_platform(&mut runtime, Arc::clone(&repo))
                    .await?
                    .into(),
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
