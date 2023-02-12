// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use clap::Args;
use futures::TryStreamExt;

/// Check a repositories internal integrity
#[derive(Debug, Args)]
pub struct CmdCheck {
    /// Trigger the check operation on a remote repository instead of the local one
    #[clap(short, long)]
    remote: Option<String>,

    /// Attempt to fix problems by pulling from another repository. Defaults to "origin".
    #[clap(long)]
    pull: Option<Option<String>>,

    /// Objects to recursively check, defaults to everything
    #[clap(name = "REF")]
    reference: Vec<String>,
}

impl CmdCheck {
    pub async fn run(&mut self, config: &spfs::Config) -> spfs::Result<i32> {
        let repo = spfs::config::open_repository_from_string(config, self.remote.as_ref()).await?;

        let pull_from = match self.pull.take() {
            Some(name @ Some(_)) if name == self.remote => {
                return Err("Cannot --pull from same repo as --remote".into());
            }
            Some(None)
                if self
                    .remote
                    .as_ref()
                    .map(|r| r == "origin")
                    .unwrap_or_default() =>
            {
                return Err("Cannot --pull from same repo as --remote".into());
            }
            Some(mut repo) => Some(
                spfs::config::open_repository_from_string(
                    config,
                    repo.take().or_else(|| Some("origin".to_owned())),
                )
                .await?,
            ),
            None => None,
        };

        let mut checker =
            spfs::Checker::new(&repo).with_reporter(spfs::check::ConsoleCheckReporter::default());
        if let Some(pull_from) = &pull_from {
            checker = checker.with_repair_source(pull_from);
        }
        let mut summary = spfs::check::CheckSummary::default();
        if self.reference.is_empty() {
            summary = checker
                .check_all_objects()
                .await?
                .into_iter()
                .map(|r| r.summary())
                .sum();
        } else {
            let mut futures: futures::stream::FuturesUnordered<_> = self
                .reference
                .iter()
                .map(|reference| checker.check_ref(reference))
                .collect();
            while let Some(result) = futures.try_next().await? {
                summary += result.summary();
            }
        }

        drop(checker); // clean up progress bars

        for digest in summary.missing_objects.union(&summary.missing_payloads) {
            println!("Missing: {digest}");
        }
        println!("{summary:#?}");

        if summary.missing_objects.len() + summary.missing_payloads.len() != 0 {
            if pull_from.is_none() {
                tracing::info!("running with `--pull` may be able to resolve these issues")
            }
            return Ok(1);
        }
        tracing::info!("repository OK");
        Ok(0)
    }
}
