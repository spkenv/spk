// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use clap::Args;
use colored::Colorize;
use futures::TryStreamExt;
use number_prefix::NumberPrefix;

/// Check a repositories internal integrity
#[derive(Debug, Args)]
pub struct CmdCheck {
    /// Trigger the check operation on a remote repository instead of the local one
    #[clap(short, long)]
    remote: Option<String>,

    /// The maximum number of tag streams that can be read and processed at once
    #[clap(long, default_value_t = spfs::Checker::DEFAULT_MAX_TAG_STREAM_CONCURRENCY)]
    max_tag_stream_concurrency: usize,

    /// The maximum number of objects that can be validated at once
    #[clap(long, default_value_t = spfs::Checker::DEFAULT_MAX_OBJECT_CONCURRENCY)]
    max_object_concurrency: usize,

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
        let start = std::time::Instant::now();
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
        let duration = std::time::Instant::now() - start;

        drop(checker); // clean up progress bars
        let spfs::check::CheckSummary {
            missing_tags,
            checked_tags,
            missing_objects,
            repaired_objects,
            checked_objects,
            missing_payloads,
            repaired_payloads,
            checked_payloads,
            checked_payload_bytes,
        } = summary;
        let missing_objects = missing_objects.len();
        let missing_payloads = missing_payloads.len();

        println!("{} after {duration:.0?}:", "Finished".bold());
        let missing = "missing".red().italic();
        let repaired = "repaired".cyan().italic();
        println!("{checked_tags:>12} tags visited     ({missing_tags} {missing})");
        println!(
            "{checked_objects:>12} objects visited  ({missing_objects} {missing}, {repaired_objects} {repaired})",
        );
        println!(
            "{checked_payloads:>12} payloads visited ({missing_payloads} {missing}, {repaired_payloads} {repaired})",
        );
        let human_bytes = match NumberPrefix::binary(checked_payload_bytes as f64) {
            NumberPrefix::Standalone(amt) => format!("{amt} bytes"),
            NumberPrefix::Prefixed(p, amt) => format!("{amt:.2} {}B", p.symbol()),
        };
        println!("{human_bytes:>12} total payload footprint");

        if missing_objects + missing_payloads != 0 {
            if pull_from.is_none() {
                tracing::info!("running with `--pull` may be able to resolve these issues")
            }
            return Ok(1);
        }
        println!("No issues found");
        Ok(0)
    }
}
