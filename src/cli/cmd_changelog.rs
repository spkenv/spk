// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

// use std::{collections::BTreeSet};
use std::{collections::HashMap, str::FromStr};
use anyhow::Result;
use clap::Args;
use colored::Colorize;

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
        let range_in_seconds: HashMap<&str, i64> = HashMap::from([
            ("d", 86400), // day
            ("w", 604800), // week
            ("m", 2592000), // month
            ("y", 31104000), // year
        ]);
        
        let changelog_range: i64 = match &self.range {
            None => *range_in_seconds.get("m").unwrap(),
            Some(range) => {
                let range_vec = range.split_terminator("").skip(1).collect::<Vec<&str>>();
                let range_multiplier = atoi::<i64>(range).unwrap();
                let range_type = *range_in_seconds.get(range_vec.last().unwrap()).unwrap();
                range_multiplier * range_type  
            }
        };
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
                    if diff < changelog_range{
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

// https://stackoverflow.com/questions/65601579/parse-an-integer-ignoring-any-non-numeric-suffix
fn atoi<F: FromStr>(input: &str) -> Result<F, <F as FromStr>::Err> {
    let i = input.find(|c: char| !c.is_numeric()).unwrap_or_else(|| input.len());
    input[..i].parse::<F>()
}

impl CommandArgs for ChangeLog {
    fn get_positional_args(&self) -> Vec<String> {
        match &self.range {
            Some(range) => vec![range.clone()],
            None => vec![],
        }
    }
}