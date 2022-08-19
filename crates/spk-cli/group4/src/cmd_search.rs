// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use anyhow::Result;
use clap::Args;
use spk_cli_common::{flags, CommandArgs, Run};
use spk_foundation::format::FormatIdent;
use spk_ident::parse_ident;

/// Search for packages by name/substring
#[derive(Args)]
pub struct Search {
    #[clap(flatten)]
    pub repos: flags::Repositories,

    #[clap(short, long, global = true, parse(from_occurrences))]
    pub verbose: u32,

    /// The text/substring to search for in package names
    term: String,
}

#[async_trait::async_trait]
impl Run for Search {
    async fn run(&mut self) -> Result<i32> {
        let repos = self.repos.get_repos_for_non_destructive_operation().await?;

        let width = repos
            .iter()
            .map(|(n, _)| n)
            .map(String::len)
            .max()
            .unwrap_or_default();
        let mut exit = 1;
        for (repo_name, repo) in repos.iter() {
            for name in repo.list_packages().await? {
                if !name.as_str().contains(&self.term) {
                    continue;
                }
                let mut ident = parse_ident(&name)?;
                let versions = repo.list_package_versions(&name).await?;
                for v in versions.iter() {
                    ident.version = (**v).clone();
                    exit = 0;
                    println!("{repo_name: <width$} {}", ident.format_ident());
                }
            }
        }
        Ok(exit)
    }
}

impl CommandArgs for Search {
    fn get_positional_args(&self) -> Vec<String> {
        // The important positional arg for a search is the search term
        vec![self.term.clone()]
    }
}
