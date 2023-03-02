// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

#![deny(unsafe_op_in_unsafe_fn)]
#![warn(clippy::fn_params_excessive_bools)]

use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use clap::Args;
use spk_cli_common::{build_required_packages, flags, CommandArgs, Run};
use spk_exec::resolve_runtime_layers;

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
        let mut solver = self.solver.get_solver(&self.options).await?;

        let requests = self
            .requests
            .parse_requests(&self.packages, &self.options, solver.repositories())
            .await?;
        for name in requests {
            solver.add_request(name);
        }

        let formatter = self.formatter_settings.get_formatter(self.verbose);
        let (solution, _) = formatter.run_and_print_resolve(&solver).await?;

        let solution = build_required_packages(&solution).await?;
        let stack = resolve_runtime_layers(&solution).await?;
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
        let config = spfs::load_config().context("Failed to load spfs config")?;
        let repo = config
            .get_local_repository()
            .await
            .context("Failed to open local spfs repo")?;
        spfs::storage::fs::Renderer::new(&repo)
            .with_reporter(spfs::storage::fs::ConsoleRenderReporter::default())
            .render_into_directory(stack, &path, spfs::storage::fs::RenderType::Copy)
            .await?;
        tracing::info!("Render completed: {path:?}");
        Ok(0)
    }
}

impl CommandArgs for Render {
    fn get_positional_args(&self) -> Vec<String> {
        /*
        The important positional args for a render are the packages
        */
        self.packages.clone()
    }
}
