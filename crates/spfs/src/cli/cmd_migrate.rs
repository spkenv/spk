// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use structopt::StructOpt;

use spfs;

#[derive(Debug, StructOpt)]
pub struct CmdMigrate {
    #[structopt(
        long = "upgrade",
        about = "Replace old data with migrated data one complete"
    )]
    upgrade: bool,
    #[structopt(about = "The path to the filesystem repository to migrate")]
    path: String,
}

impl CmdMigrate {
    pub fn run(&mut self, _config: &spfs::Config) -> spfs::Result<i32> {
        let repo_root = std::path::PathBuf::from(&self.path).canonicalize()?;
        let result = if self.upgrade {
            spfs::storage::fs::migrations::upgrade_repo(repo_root)?
        } else {
            spfs::storage::fs::migrations::migrate_repo(repo_root)?
        };
        tracing::info!(path = ?result, "migrated");
        Ok(0)
    }
}
