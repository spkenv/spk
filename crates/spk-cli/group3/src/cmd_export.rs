// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::sync::Arc;

use clap::{Args, ValueHint};
use colored::Colorize;
use miette::{Result, bail};
use spk_cli_common::{CommandArgs, Run, flags};
use spk_storage as storage;

#[cfg(test)]
#[path = "./cmd_export_test.rs"]
mod cmd_export_test;

/// Export a package as a tar file
#[derive(Args)]
pub struct Export {
    #[clap(flatten)]
    pub repos: flags::Repositories,
    #[clap(flatten)]
    pub options: flags::Options,
    #[clap(flatten)]
    pub requests: flags::Requests,

    #[clap(short, long, global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// The package to export
    #[clap(name = "PKG")]
    pub package: String,

    /// The file to export into (Defaults to the name and version of the package)
    #[arg(value_hint = ValueHint::FilePath, value_name = "FILE")]
    pub filename: Option<std::path::PathBuf>,
}

#[async_trait::async_trait]
impl Run for Export {
    type Output = i32;

    async fn run(&mut self) -> Result<Self::Output> {
        let options = self.options.get_options()?;

        let names_and_repos = self.repos.get_repos_for_non_destructive_operation().await?;
        let repo_handles = names_and_repos
            .into_iter()
            .map(|(_, r)| Arc::new(r))
            .collect::<Vec<_>>();
        let repos = repo_handles
            .iter()
            .map(|repo| match &**repo {
                storage::RepositoryHandle::SPFS(repo) => Ok(repo),
                storage::RepositoryHandle::Mem(_) | storage::RepositoryHandle::Runtime(_) => {
                    bail!("Only spfs repositories are supported")
                }
            })
            .collect::<Result<Vec<_>>>()?;

        let pkg = self
            .requests
            .parse_idents(&options, [self.package.as_str()], repo_handles.as_slice())
            .await?
            .pop()
            .unwrap();

        let mut build = String::new();
        if let Some(b) = pkg.build() {
            build = format!("_{b}");
        }
        let filename = self.filename.clone().unwrap_or_else(|| {
            std::path::PathBuf::from(format!("{}_{}{build}.spk", pkg.name(), pkg.version()))
        });
        let res = storage::export_package(repos.as_slice(), &pkg, &filename).await;
        if let Err(spk_storage::Error::PackageNotFound(_)) = res {
            tracing::warn!("Ensure that you are specifying at least a package and");
            tracing::warn!("version number when exporting from the local repository");
        }
        if res.is_err()
            && let Err(err) = std::fs::remove_file(&filename)
        {
            tracing::warn!(?err, path=?filename, "failed to clean up incomplete archive");
        }
        res?;
        println!("{}: {:?}", "Created".green(), filename);
        Ok(0)
    }
}

impl CommandArgs for Export {
    fn get_positional_args(&self) -> Vec<String> {
        // The important positional args for an export are the packages
        vec![self.package.clone()]
    }
}
