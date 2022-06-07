// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use clap::Args;

/// List the contents of a committed directory
#[derive(Debug, Args)]
#[clap(visible_aliases = &["list-dir", "list"])]
pub struct CmdLs {
    /// List files on a remote repository instead of the local one
    #[clap(long, short)]
    remote: Option<String>,

    /// The tag or digest of the file tree to read from
    #[clap(value_name = "REF")]
    reference: String,

    /// The subdirectory to list
    #[clap(default_value = "/spfs")]
    path: String,
}

impl CmdLs {
    pub async fn run(&mut self, config: &spfs::Config) -> spfs::Result<i32> {
        let repo = spfs::config::open_repository_from_string(config, self.remote.as_ref()).await?;

        let item = repo.read_ref(self.reference.as_str()).await?;

        let path = self
            .path
            .strip_prefix("/spfs")
            .unwrap_or(&self.path)
            .to_string();
        let manifest = spfs::compute_object_manifest(item, &repo).await?;
        if let Some(entries) = manifest.list_dir(path.as_str()) {
            for name in entries {
                println!("{name}");
            }
        } else {
            match manifest.get_path(path.as_str()) {
                None => {
                    tracing::error!("path not found in manifest: {}", self.path);
                }
                Some(_entry) => {
                    tracing::error!("path is not a directory: {}", self.path);
                }
            }
            return Ok(1);
        }
        Ok(0)
    }
}
