// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use clap::Args;
use miette::Result;
use spfs::Error;
use spfs::prelude::*;
use spfs_cli_common as cli;

/// Output the contents of a blob to stdout
#[derive(Debug, Args)]
#[clap(visible_aliases = &["read-file", "cat", "cat-file"])]
pub struct CmdRead {
    #[clap(flatten)]
    pub(crate) repos: cli::Repositories,

    /// The tag or digest of the blob/payload to output
    #[clap(value_name = "REF")]
    reference: spfs::tracking::EnvSpecItem,

    /// If the given ref is not a blob, read the blob found at this path
    #[clap(value_name = "PATH")]
    path: Option<String>,
}

impl CmdRead {
    pub async fn run(&mut self, config: &spfs::Config) -> Result<i32> {
        let repo =
            spfs::config::open_repository_from_string(config, self.repos.remote.as_ref()).await?;

        #[cfg(feature = "sentry")]
        tracing::info!(target: "sentry", "using repo: {}", repo.address());

        let item = repo.read_ref(&self.reference.to_string()).await?;
        use spfs::graph::object::Enum;
        let blob = match item.to_enum() {
            Enum::Blob(blob) => blob,
            _ => {
                let path = match &self.path {
                    None => {
                        miette::bail!("PATH must be given to read from {:?}", item.kind());
                    }
                    Some(p) => p.strip_prefix("/spfs").unwrap_or(p).to_string(),
                };
                let manifest = spfs::compute_object_manifest(item, &repo).await?;
                let entry = match manifest.get_path(&path) {
                    Some(e) => e,
                    None => {
                        tracing::error!("file does not exist: {path}");
                        return Ok(1);
                    }
                };
                if !entry.kind.is_blob() {
                    tracing::error!("path is a directory or masked file: {path}");
                    return Ok(1);
                }
                repo.read_blob(entry.object).await?
            }
        };

        let (mut payload, filename) = repo.open_payload(*blob.digest()).await?;
        tokio::io::copy(&mut payload, &mut tokio::io::stdout())
            .await
            .map_err(|err| Error::StorageReadError("copy of payload to stdout", filename, err))?;
        Ok(0)
    }
}
