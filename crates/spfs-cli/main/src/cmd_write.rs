// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

#[cfg(unix)]
use std::os::unix::prelude::PermissionsExt;
use std::path::PathBuf;

use anyhow::Result;
use clap::Args;
use spfs::tracking::BlobReadExt;
use spfs::Error;

/// Store an arbitrary blob of data in spfs
#[derive(Debug, Args)]
#[clap(visible_aliases = &["write-file"])]
pub struct CmdWrite {
    /// A human-readable tag for the generated object
    ///
    /// Can be provided more than once.
    #[clap(long = "tag", short)]
    tags: Vec<String>,

    /// Write to a remote repository instead of the local one
    #[clap(long, short)]
    remote: Option<String>,

    /// Store the contents of this file instead of reading from stdin
    #[clap(long, short)]
    file: Option<PathBuf>,
}

impl CmdWrite {
    pub async fn run(&mut self, config: &spfs::Config) -> Result<i32> {
        let repo = spfs::config::open_repository_from_string(config, self.remote.as_ref()).await?;

        let reader: std::pin::Pin<Box<dyn spfs::tracking::BlobRead>> = match &self.file {
            Some(file) => {
                let handle = tokio::fs::File::open(&file)
                    .await
                    .map_err(|err| Error::RuntimeWriteError(file.clone(), err))?;
                #[cfg(unix)]
                let mode = handle
                    .metadata()
                    .await
                    .map_err(|err| Error::RuntimeWriteError(file.clone(), err))?
                    .permissions()
                    .mode();
                #[cfg(windows)]
                let mode = 0o644;
                Box::pin(tokio::io::BufReader::new(handle).with_permissions(mode))
            }
            None => Box::pin(tokio::io::BufReader::new(tokio::io::stdin())),
        };

        let digest = repo.commit_blob(reader).await?;

        tracing::info!(?digest, "created");
        for tag in self.tags.iter() {
            let tag_spec = match spfs::tracking::TagSpec::parse(tag) {
                Ok(tag_spec) => tag_spec,
                Err(err) => {
                    tracing::warn!("cannot set invalid tag '{tag}': {err:?}");
                    continue;
                }
            };
            repo.push_tag(&tag_spec, &digest).await?;
            tracing::info!(?tag, "created");
        }

        Ok(0)
    }
}
