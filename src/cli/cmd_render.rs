// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use clap::Args;

use super::{flags, Run};

/// Output the contents of an spk environment (/spfs) to a folder
#[derive(Args)]
pub struct Render {
    #[clap(flatten)]
    pub solver: flags::Solver,
    #[clap(flatten)]
    pub options: flags::Options,
    #[clap(flatten)]
    pub requests: flags::Requests,

    #[clap(short, long, global = true, parse(from_occurrences))]
    pub verbose: u32,

    #[clap(flatten)]
    pub formatter_settings: flags::DecisionFormatterSettings,

    /// The packages to resolve and render
    #[clap(name = "PKG", required = true)]
    packages: Vec<String>,

    /// The empty directory to render into
    #[clap(name = "PATH")]
    target: PathBuf,
}

#[async_trait::async_trait]
impl Run for Render {
    async fn run(&mut self) -> Result<i32> {
        let (mut solver, requests) = tokio::try_join!(
            self.solver.get_solver(&self.options),
            self.requests.parse_requests(&self.packages, &self.options)
        )?;
        for name in requests {
            solver.add_request(name);
        }

        let formatter = self.formatter_settings.get_formatter(self.verbose);
        let solution = formatter.run_and_print_resolve(&solver).await?;

        let solution = spk::build_required_packages(&solution).await?;
        let stack = spk::exec::resolve_runtime_layers(&solution).await?;
        std::fs::create_dir_all(&self.target).context("Failed to create output directory")?;
        if std::fs::read_dir(&self.target)
            .context("Failed to validate output directory")?
            .next()
            .is_some()
        {
            bail!("Output directory does not appear to be empty");
        }

        let path = self.target.canonicalize()?;
        tracing::info!("Rendering into dir: {path:?}");
        let items: Vec<String> = stack.iter().map(ToString::to_string).collect();
        let env_spec = items.join("+").parse()?;
        spfs::render_into_directory(&env_spec, &path).await?;
        tracing::info!("Render completed: {path:?}");
        Ok(0)
    }
}
