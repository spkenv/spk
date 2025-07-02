// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::path::PathBuf;

use clap::{Args, Subcommand};
use miette::Result;

/// Create an empty filesystem repository
#[derive(Debug, Args)]
pub struct CmdInit {
    #[clap(subcommand)]
    cmd: InitSubcommand,
}

impl CmdInit {
    pub async fn run(&self, config: &spfs::Config) -> Result<i32> {
        self.cmd.run(config).await
    }
}

#[derive(strum::AsRefStr, Debug, Subcommand)]
#[strum(serialize_all = "lowercase")]
pub enum InitSubcommand {
    /// Initialize an empty filesystem repository
    ///
    /// Does nothing when run on an existing repository
    Repo {
        /// The root of the new repository
        path: PathBuf,
    },
}

impl InitSubcommand {
    pub async fn run(&self, _config: &spfs::Config) -> Result<i32> {
        match self {
            Self::Repo { path } => {
                spfs::storage::fs::MaybeOpenFsRepository::create(&path).await?;
                Ok(0)
            }
        }
    }
}
