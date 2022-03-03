// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use structopt::StructOpt;

#[derive(Debug, StructOpt)]
pub struct CmdRead {
    #[structopt(
        long = "remote",
        short = "r",
        about = "Read from a remote repository instead of the local one"
    )]
    remote: Option<String>,
    #[structopt(
        value_name = "REF",
        about = "The tag or digest of the blob/payload to output"
    )]
    reference: String,
    #[structopt(
        value_name = "PATH",
        about = "If the given ref is not a blob, read the blob found at this path"
    )]
    path: Option<String>,
}

impl CmdRead {
    pub async fn run(&mut self, config: &spfs::Config) -> spfs::Result<i32> {
        let repo = match &self.remote {
            Some(remote) => config.get_remote(remote).await?,
            None => config.get_repository().await?.into(),
        };
        let item = repo.read_ref(self.reference.as_str()).await?;
        use spfs::graph::Object;
        let blob = match item {
            Object::Blob(blob) => blob,
            _ => {
                let path = match &self.path {
                    None => {
                        return Err(
                            format!("PATH must be given to read from {:?}", item.kind()).into()
                        )
                    }
                    Some(p) => p.strip_prefix("/spfs").unwrap_or(p).to_string(),
                };
                let manifest = spfs::compute_object_manifest(item, &repo).await?;
                let entry = match manifest.get_path(&path) {
                    Some(e) => e,
                    None => {
                        tracing::error!("file does not exist: {}", path);
                        return Ok(1);
                    }
                };
                if !entry.kind.is_blob() {
                    tracing::error!("path is a directory or masked file: {}", path);
                    return Ok(1);
                }
                repo.read_blob(entry.object).await?
            }
        };

        let mut payload = repo.open_payload(blob.digest()).await?;
        tokio::io::copy(&mut payload, &mut tokio::io::stdout()).await?;
        Ok(0)
    }
}
