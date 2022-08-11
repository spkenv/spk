// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use anyhow::Result;
use clap::Args;
use colored::Colorize;
use std::{collections::HashMap, str::FromStr, sync::Arc};

use super::{flags, CommandArgs, Run};

use chrono::{DateTime, Local, NaiveDateTime, Utc};
#[derive(Args)]
pub struct ChangeLog {
    #[clap(flatten)]
    pub repos: flags::Repositories,

    /// The range in d(day), w(week), m(month), or y(year) you want to check changes
    #[clap(name = "time", short)]
    pub time: Option<String>,

    #[clap(name = "NAME[/VERSION]")]
    package: Option<String>,
}

#[async_trait::async_trait]
impl Run for ChangeLog {
    async fn run(&mut self) -> Result<i32> {
        let mut repos = self.repos.get_repos_for_non_destructive_operation().await?;
        if repos.is_empty() {
            let local = String::from("local");
            if !self.repos.disable_repo.contains(&local) {
                repos = self.repos.get_repos_for_non_destructive_operation().await?;
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
            ("d", 86400),    // day
            ("w", 604800),   // week
            ("m", 2592000),  // month
            ("y", 31104000), // year
        ]);
        let changelog_range: i64 = match &self.time {
            None => *range_in_seconds.get("m").unwrap(),
            Some(time) => {
                let range_vec = time.split_terminator("").skip(1).collect::<Vec<&str>>();
                let range_multiplier = atoi::<i64>(time).unwrap();
                let range_type = *range_in_seconds.get(range_vec.last().unwrap()).unwrap();
                range_multiplier * range_type
            }
        };

        match &self.package {
            None => {
                for (index, (_, repo)) in repos.iter().enumerate() {
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
                            let mut spec = match repo.read_spec(&ident).await {
                                Ok(s) => s,
                                Err(err) => {
                                    tracing::debug!(
                                        "Unable to read {ident} spec from {_repo_name}: {err}"
                                    );
                                    continue;
                                }
                            };

                            let recent_change =
                                Arc::make_mut(&mut spec).meta.get_recent_modified_time();
                            let current_time = chrono::offset::Local::now().timestamp();
                            let diff = current_time - recent_change.timestamp;

                            if diff < changelog_range {
                                let naive_date_time =
                                    NaiveDateTime::from_timestamp(recent_change.timestamp, 0);
                                let date_time = DateTime::<Utc>::from_utc(naive_date_time, Utc)
                                    .with_timezone(&Local);
                                println!(
                                    "Package: {}, Modified on {}",
                                    name.yellow(),
                                    date_time.to_string().yellow()
                                );
                                println!(
                                    "Author: {}, Action: {}, Comment: {}",
                                    recent_change.author.yellow(),
                                    recent_change.action.yellow(),
                                    recent_change.comment.yellow(),
                                );
                            }
                            println!();
                        }
                    }
                }
            }
            Some(package) => {
                if !package.contains('/') {
                    tracing::error!("Must provide a version number: {package}/<VERSION NUMBER>");
                    tracing::error!(
                        " > use 'spk ls {package}' or 'spk ls {package} -r <REPO_NAME>' to view available versions"
                    );
                    return Ok(2);
                }
                if package.ends_with('/') {
                    tracing::error!("A trailing '/' isn't a valid version number or build digest in '{package}'. Please remove the trailing '/', or specify a version number or build digest after it.");
                    return Ok(3);
                }
                let ident = spk::api::parse_ident(package)?;
                for (repo_name, repo) in repos.iter() {
                    let mut spec = match repo.read_spec(&ident).await {
                        Ok(s) => s,
                        Err(err) => {
                            tracing::debug!("Unable to read {ident} spec from {repo_name}: {err}");
                            continue;
                        }
                    };

                    for change in &Arc::make_mut(&mut spec).meta.modified_stack {
                        let current_time = chrono::offset::Local::now().timestamp();
                        let diff = current_time - change.timestamp;

                        if diff < changelog_range {
                            let naive_date_time =
                                NaiveDateTime::from_timestamp(change.timestamp, 0);
                            let date_time = DateTime::<Utc>::from_utc(naive_date_time, Utc)
                                .with_timezone(&Local);
                            println!(
                                "Package: {}, Modified on {}",
                                ident.to_string().yellow(),
                                date_time.to_string().yellow()
                            );
                            println!(
                                "Author: {}, Action: {}, Comment: {}",
                                change.author.yellow(),
                                change.action.yellow(),
                                change.comment.yellow(),
                            );
                        }
                        println!();
                    }
                }
            }
        }
        Ok(0)
    }
}

// https://stackoverflow.com/questions/65601579/parse-an-integer-ignoring-any-non-numeric-suffix
fn atoi<F: FromStr>(input: &str) -> Result<F, <F as FromStr>::Err> {
    let i = input.find(|c: char| !c.is_numeric()).unwrap_or(input.len());
    input[..i].parse::<F>()
}

impl CommandArgs for ChangeLog {
    fn get_positional_args(&self) -> Vec<String> {
        match &self.time {
            Some(time) => vec![time.clone()],
            None => vec![],
        }
    }
}
