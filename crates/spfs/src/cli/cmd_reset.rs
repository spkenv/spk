// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use clap::Args;
use spfs::prelude::*;

/// Reset changes, or rebuild the entire spfs directory
#[derive(Args, Debug)]
pub struct CmdReset {
    /// Mount the resulting runtime in edit mode
    ///
    /// Default to true if REF is empty or not given
    #[clap(short, long)]
    edit: bool,

    /// The tag or id to rebuild the runtime with.
    ///
    /// Uses current runtime stack if not given. Use '-' or
    /// an empty string to request an empty environment. Only valid
    /// if no paths are given
    #[clap(long = "ref", short)]
    reference: Option<String>,

    /// Glob patterns in the spfs dir of files to reset, defaults to everything
    paths: Vec<String>,
}

impl CmdReset {
    pub async fn run(&mut self, config: &spfs::Config) -> spfs::Result<i32> {
        let mut runtime = spfs::active_runtime().await?;
        let repo = config.get_local_repository().await?;
        if let Some(reference) = &self.reference {
            runtime.reset::<&str>(&[])?;
            runtime.status.stack.truncate(0);
            match reference.as_str() {
                "" | "-" => self.edit = true,
                _ => {
                    let env_spec = spfs::tracking::EnvSpec::parse(reference)?;
                    for target in env_spec.iter() {
                        let obj = repo.read_ref(target.to_string().as_ref()).await?;
                        runtime.push_digest(&obj.digest()?);
                    }
                }
            }
        } else {
            let paths = strip_spfs_prefix(&self.paths);
            runtime.reset(paths.as_slice())?;
        }

        if self.edit {
            runtime.status.editable = true;
        }

        runtime.save_state_to_storage().await?;
        spfs::remount_runtime(&runtime).await?;
        Ok(0)
    }
}

fn strip_spfs_prefix(paths: &[String]) -> Vec<String> {
    paths
        .iter()
        .map(|path| {
            path.strip_prefix("/spfs")
                .unwrap_or_else(|| path.as_ref())
                .to_owned()
        })
        .collect()
}
