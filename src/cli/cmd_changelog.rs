// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::{collections::BTreeSet};
use anyhow::{Context, Result};
use clap::Args;
use colored::Colorize;

use super::{flags, CommandArgs, Run};

#[derive(Args)]
pub struct ChangeLog {

    #[clap(flatten)]
    pub repos: flags::Repositories,

    #[clap(name = "d|w|mo")]
    pub range: Option<String>,

    #[clap(name = "NAME[/VERSION]")]
    package: Option<String>,
}

#[async_trait::async_trait]
impl Run for ChangeLog {
    async fn run(&mut self) -> Result<i32> {
        let mut repos = self.repos.get_repos(None).await?;

        if repos.is_empty() {
            let local = String::from("local");
            if !self.repos.disable_repo.contains(&local) {
                repos = self.repos.get_repos(None).await?;
            } else {
                eprintln!(
                    "{}",
                    "No repositories selected, specify --local-repo (-l) and/or --enable-repo (-r)"
                        .yellow()
                );
                return Ok(1);
            }
        }

        let mut results: Vec<String> = Vec::new();
        let mut set: BTreeSet<String> = BTreeSet::new();  
        for (_repo_name, repo) in repos {
            set.extend(
                repo.list_packages()
                    .await?
                    .into_iter()
                    .map(spk::api::PkgNameBuf::into),
            )
        }
        results = set.into_iter().collect();
        for item in results {
            let pkg: Option<String> = Some(item); 
            let (_, spec) = flags::find_package_spec(&pkg)
                .context("find package spec")?
                .must_be_found();

            println!("{:?}", spec.meta.creation_timestamp);
        }
        Ok(0)

    }
}

impl CommandArgs for ChangeLog {
    fn get_positional_args(&self) -> Vec<String> {
        match &self.range {
            Some(range) => vec![range.clone()],
            None => vec!["1mo".into()],
        }
    }
}