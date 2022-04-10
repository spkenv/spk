// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::path::PathBuf;

use anyhow::{anyhow, Context, Result};
use clap::Args;

use super::flags;

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

    /// The packages to resolve and render
    #[clap(name = "PKG", required = true)]
    packages: Vec<String>,

    /// The empty directory to render into
    #[clap(name = "PATH")]
    target: PathBuf,
}

impl Render {
    pub fn run(&self) -> Result<i32> {
        let mut solver = self.solver.get_solver(&self.options)?;
        for name in self
            .requests
            .parse_requests(&self.packages, &self.options)?
        {
            solver.add_request(name);
        }

        let solution = spk::io::run_and_print_resolve(&solver, self.verbose)?;

        let solution = spk::build_required_packages(&solution)?;
        let stack = spk::exec::resolve_runtime_layers(&solution)?;
        std::fs::create_dir_all(&self.target).context("Failed to create output directory")?;
        if std::fs::read_dir(&self.target)
            .context("Failed to validate output directory")?
            .next()
            .is_some()
        {
            return Err(anyhow!("Output directory does not appear to be empty"));
        }

        let path = self.target.canonicalize()?;
        tracing::info!("Rendering into dir: {path:?}");
        let items: Vec<String> = stack.iter().map(ToString::to_string).collect();
        let env_spec = spfs::tracking::EnvSpec::new(items.join("+").as_ref())?;
        spk::HANDLE.block_on(spfs::render_into_directory(&env_spec, &path))?;
        tracing::info!("Render completed: {path:?}");
        Ok(0)
    }
}
