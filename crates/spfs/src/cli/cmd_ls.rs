// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use structopt::StructOpt;

#[derive(Debug, StructOpt)]
pub struct CmdLs {
    #[structopt(
        long = "remote",
        short = "r",
        about = "Operate on a remote repository instead of the local one"
    )]
    remote: Option<String>,
    #[structopt(
        value_name = "REF",
        about = "The tag or digest of the file tree to read from"
    )]
    reference: String,
    #[structopt(
        default_value = "/",
        about = "The subdirectory to list, defaults to the root ('/spfs')"
    )]
    path: String,
}

impl CmdLs {
    pub async fn run(&mut self, config: &spfs::Config) -> spfs::Result<i32> {
        let repo = match &self.remote {
            Some(remote) => config.get_remote(remote).await?,
            None => config.get_repository().await?.into(),
        };
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
