// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

// use std::{collections::BTreeSet};
use anyhow::Result;
use clap::Args;
use colored::Colorize;
use regex;

use chrono::{NaiveDateTime, DateTime, Utc, Local};
use super::{flags, CommandArgs, Run};

#[derive(Args)]
pub struct ChangeLog {

    #[clap(flatten)]
    pub repos: flags::Repositories,

    #[clap(name = "D|W|M")]
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

        // Seconds in day, week, month, and year for comparison when checking creation date.
        let _day: i64 = 86400;
        let _week: i64 = 604800;
        let _month: i64 = 2592000;
        let _year: i64 = 31104000;

        // Work in arguments
        let mut res = i64::default();
        match &self.range {
            None => {
                res = _month;
                println!("{:?}", res);
            }
            Some(range) => {
                println!("{:?}", range);
            }
        }

        for (index, (_, repo)) in repos.iter().enumerate()  {
            let packages = repo.list_packages().await?;
            for package in packages {

                let mut versions = Vec::new();
                versions.extend(
                    repo.list_package_versions(&package.clone())
                        .await?
                        .iter()
                        .map(|v| ((**v).clone(), index)),
                );
                versions.sort_by_key(|v| v.0.clone());
                versions.reverse();

                for (version, repo_index) in versions {
                    let (_repo_name, repo) = repos.get(repo_index).unwrap();
                    let mut name = String::from(&package.to_string());
                    name.push('/');
                    name.push_str(&version.to_string());
                    
                    let ident = spk::api::parse_ident(name.clone())?;
                    let spec = repo.read_spec(&ident).await?;

                    
                    let current_time = chrono::offset::Local::now().timestamp();
                    let diff = current_time - spec.meta.creation_timestamp;
                    if diff < res{
                        let naive_date_time = NaiveDateTime::from_timestamp(spec.meta.creation_timestamp, 0);
                        let date_time = DateTime::<Utc>::from_utc(naive_date_time, Utc).with_timezone(&Local);
                        println!("Package {}: Created on {}", name, date_time);
                    }
                }
            }

        }
        Ok(0)

    }
}

impl CommandArgs for ChangeLog {
    fn get_positional_args(&self) -> Vec<String> {
        match &self.range {
            Some(range) => vec![range.clone()],
            None => vec![],
        }
    }
}