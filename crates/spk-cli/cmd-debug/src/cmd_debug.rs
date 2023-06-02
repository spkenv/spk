// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::HashMap;
use std::convert::TryInto;

use anyhow::Result;
use clap::Args;
use futures::TryFutureExt;
use spk_cli_common::{current_env, flags, CommandArgs, Run};
use spk_schema::foundation::format::FormatIdent;
use spk_schema::ident_build::Build;
use spk_storage::{self as storage};

/// Install source packages of the packages in the current environment
#[derive(Args)]
pub struct Debug {
    #[clap(flatten)]
    pub solver: flags::Solver,
    #[clap(flatten)]
    pub options: flags::Options,

    #[clap(short, long, global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,
}

#[async_trait::async_trait]
impl Run for Debug {
    async fn run(&mut self) -> Result<i32> {
        let (env, mut repos, local_repo) = tokio::try_join!(
            current_env().map_err(|err| err.into()),
            self.solver.repos.get_repos_for_non_destructive_operation(),
            storage::local_repository().map_err(|err| err.into()),
        )?;

        // Move the local repo to the front of `repos` (assuming it is enabled)
        // so devs pick up their local src package in the situation where they
        // are developing and haven't changed the package version number from
        // an existing published package.
        repos.sort_unstable_by_key(|(repo_name, _)| i32::from(repo_name != "local"));

        let mut source_layers = HashMap::new();

        'next_request: for solved in env.items() {
            if let Some(Build::Digest(_)) = solved.request.pkg.build.as_ref() {
                let mut source_pkg = solved.request.pkg.clone();
                source_pkg.build = Some(Build::Source);

                if let Ok(ident) = source_pkg.try_into() {
                    // Search for a repo that has this source package.
                    // TODO: It would be useful if it was possible to know what repo
                    // a package found in runtime repo came from.
                    for (_, repo) in repos.iter() {
                        if let Ok(comps) = repo.read_components(&ident).await {
                            if self.verbose > 0 && !comps.is_empty() {
                                tracing::info!("Adding source package: {}", ident.format_ident());
                            }
                            for digest in comps.values() {
                                source_layers.insert(*digest, repo);
                            }
                            continue 'next_request;
                        }
                    }

                    if self.verbose > 0 {
                        tracing::info!("No source package found for: {}", solved.request.pkg);
                    }
                }
            };
        }

        if source_layers.is_empty() {
            tracing::info!("No source packages were found for the current environment.");
            return Ok(0);
        }

        let mut rt = spfs::active_runtime().await?;

        for (layer, repo) in source_layers {
            if !local_repo.has_object(layer).await {
                if let storage::RepositoryHandle::SPFS(repo) = repo {
                    let syncer = spfs::Syncer::new(repo, &local_repo)
                        .with_reporter(spfs::sync::ConsoleSyncReporter::default());
                    syncer.sync_digest(layer).await?;
                }
            }

            rt.push_digest(layer);
        }

        rt.save_state_to_storage().await?;
        spfs::remount_runtime(&rt).await?;

        Ok(0)
    }
}

impl CommandArgs for Debug {
    fn get_positional_args(&self) -> Vec<String> {
        Vec::new()
    }
}
