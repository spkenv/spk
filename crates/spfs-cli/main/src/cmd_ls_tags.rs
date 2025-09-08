// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use clap::Args;
use miette::Result;
use relative_path::{RelativePath, RelativePathBuf};
use spfs::prelude::*;
use spfs::storage::EntryType;
use spfs_cli_common as cli;
use tokio_stream::StreamExt;

/// List tags by their path
#[derive(Debug, Args)]
#[clap(visible_aliases = &["list-tags"])]
pub struct CmdLsTags {
    #[clap(flatten)]
    pub(crate) repos: cli::Repositories,

    /// Walk the tag tree recursively listing all tags under the specified dir
    #[clap(long)]
    recursive: bool,

    /// The tag path to list under
    #[clap(default_value = "/")]
    path: String,
}

impl CmdLsTags {
    pub async fn run(&mut self, config: &spfs::Config) -> Result<i32> {
        let repo =
            spfs::config::open_repository_from_string(config, self.repos.remote.as_ref()).await?;

        let root = RelativePathBuf::from(&self.path);
        let mut dirs = std::collections::VecDeque::new();
        dirs.push_back(root.clone());
        let mut names = Vec::new();

        while let Some(dir) = dirs.pop_front() {
            let mut entries = repo.ls_tags(&dir);
            while let Some(item) = entries.next().await {
                match item {
                    Ok(item) => {
                        let path = dir.join(item.to_string());
                        let path = path
                            .strip_prefix(&root)
                            .map(RelativePath::to_owned)
                            .unwrap_or(path);
                        match item {
                            EntryType::Folder(name) if self.recursive => {
                                tracing::debug!("walking {path}...");
                                dirs.push_back(dir.join(name));
                            }
                            EntryType::Folder(_name) => names.push(path),
                            EntryType::Namespace { .. } => {
                                // Hide namespaces from end users
                                continue;
                            }
                            EntryType::Tag(_name) => names.push(path),
                        }
                    }
                    Err(err) => tracing::error!(%err, %dir, "error reading tag"),
                }
            }
        }

        names.sort();
        for name in names {
            println!("{name}")
        }

        Ok(0)
    }
}
