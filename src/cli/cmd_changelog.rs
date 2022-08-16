// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use anyhow::Result;
use clap::Args;
use colored::Colorize;
use std::sync::Arc;

use super::{flags, CommandArgs, Run};

use chrono::{DateTime, Duration, Local, NaiveDateTime, Utc};
/// Returns meta data of packages changed within a time frame
#[derive(Args)]
pub struct Changelog {
    #[clap(flatten)]
    pub repos: flags::Repositories,

    /// The range in d(day), w(week), m(month), or y(year) you want to check changes
    #[clap(name = "time", short)]
    pub time: Option<String>,

    #[clap(name = "NAME[/VERSION]")]
    package: Option<String>,
}

#[async_trait::async_trait]
impl Run for Changelog {
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

        let oldest_date_for_search =
            age_to_date(self.time.clone().unwrap_or_else(|| "1m".into()))?.timestamp();

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
                            let (repo_name, repo) = repos.get(repo_index).unwrap();
                            let mut name = String::from(&package.to_string());
                            name.push('/');
                            name.push_str(&version.to_string());

                            let ident = spk::api::parse_ident(name.clone())?;
                            let spec = match repo.read_spec(&ident).await {
                                Ok(s) => s,
                                Err(err) => {
                                    tracing::debug!(
                                        "Unable to read {ident} spec from {repo_name}: {err}"
                                    );
                                    continue;
                                }
                            };

                            if spec.meta.modification_history.is_empty() {
                                tracing::debug!("Package {ident} does not have modified meta data");
                                continue;
                            }

                            let recent_change = spec.meta.get_recent_modified_time();
                            print_modified_packages(name, oldest_date_for_search, &recent_change);
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

                    if Arc::make_mut(&mut spec)
                        .meta
                        .modification_history
                        .is_empty()
                    {
                        tracing::debug!("Package {ident} does not have modified meta data");
                        continue;
                    }

                    for change in &Arc::make_mut(&mut spec).meta.modification_history {
                        print_modified_packages(ident.to_string(), oldest_date_for_search, change);
                    }
                }
            }
        }
        Ok(0)
    }
}

fn age_to_date(age: String) -> spfs::Result<DateTime<Utc>> {
    let (num, postfix) = age.split_at(age.len() - 1);
    let num: i64 = num
        .parse()
        .map_err(|err| spfs::Error::from(format!("{:?}", err)))?;
    if num < 0 {
        return Err(format!("provided age must be greater than zero: '{age}'").into());
    }

    match postfix {
        "y" => Ok(Utc::now() - Duration::weeks(num * 52)),
        "m" => Ok(Utc::now() - Duration::weeks(num * 4)),
        "w" => Ok(Utc::now() - Duration::weeks(num)),
        "d" => Ok(Utc::now() - Duration::days(num)),
        _ => Err(format!("Unknown age postfix: '{postfix}', must be one of y, m, w, d").into()),
    }
}

fn print_modified_packages(
    pkg: String,
    oldest_date_for_search: i64,
    metadata: &spk::api::meta::ModifiedMetaData,
) {
    if oldest_date_for_search < metadata.timestamp {
        let naive_date_time = NaiveDateTime::from_timestamp(metadata.timestamp, 0);
        let date_time = DateTime::<Utc>::from_utc(naive_date_time, Utc).with_timezone(&Local);
        println!(
            "Package: {}, Modified on {}",
            pkg.yellow(),
            date_time.to_string().yellow()
        );
        println!(
            "Author: {}, Action: {}, Comment: {}",
            metadata.author.yellow(),
            metadata.action.yellow(),
            metadata.comment.yellow(),
        );
        println!();
    }
}

impl CommandArgs for Changelog {
    fn get_positional_args(&self) -> Vec<String> {
        match &self.time {
            Some(time) => vec![time.clone()],
            None => vec![],
        }
    }
}
