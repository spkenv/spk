// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use anyhow::Result;
use clap::Args;

use super::{flags, Run};

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

impl Run for Search {
    fn run(&mut self) -> Result<i32> {
        let repos = self.repos.get_repos(&["origin".to_string()])?;

        let width = repos
            .iter()
            .map(|(n, _)| n)
            .map(String::len)
            .max()
            .unwrap_or_default();
        let mut exit = 1;
        for (repo_name, repo) in repos.iter() {
            for name in repo.list_packages()? {
                if !name.contains(&self.term) {
                    continue;
                }
                let mut ident = spk::api::parse_ident(&name)?;
                let versions = repo.list_package_versions(&name)?;
                for v in versions.iter() {
                    ident.version = (**v).clone();
                    exit = 0;
                    println!("{repo_name: <width$} {}", spk::io::format_ident(&ident));
                }
            }
        }
        Ok(exit)
    }
}
