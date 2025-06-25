// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use clap::Args;
use miette::Result;
use spk_cli_common::{CommandArgs, Run, flags};

#[cfg(test)]
#[path = "./cmd_build_tree_test.rs"]
mod cmd_build_tree_test;

/// Builds the entire dependency tree of a platform using a workspace of package specs.
#[derive(Args, Clone)]
#[clap(visible_aliases = &["make-tree", "mkt", "bt"])]
pub struct BuildTree {
    #[clap(flatten)]
    runtime: flags::Runtime,
    #[clap(flatten)]
    solver: flags::Solver,
    #[clap(flatten)]
    options: flags::Options,

    #[clap(short, long, global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// Build the first variant of this package, and then immediately enter a shell environment with it
    #[clap(long, short)]
    env: bool,

    #[clap(flatten)]
    packages: flags::Packages,

    /// Build only the specified variants
    #[clap(flatten)]
    variant: flags::Variant,

    /// Allow dependencies of the package being built to have a dependency on
    /// this package.
    #[clap(long)]
    pub allow_circular_dependencies: bool,
}

#[async_trait::async_trait]
impl Run for BuildTree {
    type Output = BuildResult;

    async fn run(&mut self) -> Result<Self::Output> {
        self.runtime
            .ensure_active_runtime(&["build-tree", "make-tree", "mkt", "bt"])
            .await?;

        // divide our packages into one for each iteration of mks/mkb
        let mut runs: Vec<_> = self.packages.split();
        if runs.is_empty() {
            runs.push(Default::default());
        }

        let builds_for_summary = spk_cli_common::BuildResult::default();

        println!("Completed builds:");
        for (_, artifact) in builds_for_summary.iter() {
            println!("   {artifact}");
        }

        Ok(BuildResult {
            exit_status: 0,
            created_builds: builds_for_summary,
        })
    }
}

#[derive(Debug)]
pub struct BuildResult {
    pub exit_status: i32,
    pub created_builds: spk_cli_common::BuildResult,
}

impl From<BuildResult> for i32 {
    fn from(result: BuildResult) -> Self {
        result.exit_status
    }
}

impl CommandArgs for BuildTree {
    // The important positional args for a build are the packages
    fn get_positional_args(&self) -> Vec<String> {
        self.packages.get_positional_args()
    }
}
