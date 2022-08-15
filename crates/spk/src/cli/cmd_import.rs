// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use anyhow::{Context, Result};
use clap::Args;
use spk_storage::{self as storage};

use super::{CommandArgs, Run};

/// Import a previously exported package/archive
#[derive(Args)]
pub struct Import {
    /// The archive to import from
    #[clap(name = "FILE")]
    pub files: Vec<std::path::PathBuf>,
}

#[async_trait::async_trait]
impl Run for Import {
    async fn run(&mut self) -> Result<i32> {
        for filename in self.files.iter() {
            storage::import_package(filename)
                .await
                .context("Import failed")?;
        }
        Ok(0)
    }
}

impl CommandArgs for Import {
    fn get_positional_args(&self) -> Vec<String> {
        // The important positional args for an import are the archive files
        self.files
            .iter()
            .map(|p| format!("{}", p.display()))
            .collect::<Vec<String>>()
    }
}
