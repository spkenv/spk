// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

#![deny(unsafe_op_in_unsafe_fn)]
#![warn(clippy::fn_params_excessive_bools)]

use std::path::PathBuf;

use clap::Args;
use miette::{bail, Context, IntoDiagnostic, Result};
use spfs::storage::fallback::FallbackProxy;
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

    #[clap(short, long, global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,

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

        let formatter = self.formatter_settings.get_formatter(self.verbose)?;
        let (solution, _) = formatter.run_and_print_resolve(&solver).await?;

        let solution = build_required_packages(&solution).await?;
        let stack = resolve_runtime_layers(true, &solution).await?;
        std::fs::create_dir_all(&self.target)
            .into_diagnostic()
            .wrap_err("Failed to create output directory")?;
        if std::fs::read_dir(&self.target)
            .into_diagnostic()
            .wrap_err("Failed to validate output directory")?
            .next()
            .is_some()
        {
            bail!("Output directory does not appear to be empty");
        }

        let path = dunce::canonicalize(&self.target).into_diagnostic()?;
        tracing::info!("Rendering into dir: {path:?}");
        let config = spfs::load_config().wrap_err("Failed to load spfs config")?;
        let local = config
            .get_opened_local_repository()
            .await
            .wrap_err("Failed to open local spfs repo")?;

        // Find possible fallback repositories among the solver's repositories.
        let mut fallback_repository_handles = Vec::with_capacity(solver.repositories().len());
        for repo in solver.repositories().iter().filter(|repo| {
            repo.is_spfs() && {
                // XXX: is there a better way to identify the local repo?
                repo.name() != "local"
            }
        }) {
            // XXX: Can't find a better way to get an owned RepositoryHandle
            // from the &Arc<RepositoryHandle> inside the Solver.
            if let Ok(handle) = spfs::open_repository(repo.address()).await {
                fallback_repository_handles.push(handle);
            }
        }

        if fallback_repository_handles.is_empty() {
            spfs::storage::fs::Renderer::new(&local)
                .with_reporter(spfs::storage::fs::ConsoleRenderReporter::default())
                .render_into_directory(stack, &path, spfs::storage::fs::RenderType::Copy)
                .await?;
        } else {
            let fallback = FallbackProxy::new(local, fallback_repository_handles);
            spfs::storage::fs::Renderer::new(&fallback)
                .with_reporter(spfs::storage::fs::ConsoleRenderReporter::default())
                .render_into_directory(stack, &path, spfs::storage::fs::RenderType::Copy)
                .await?;
        }

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
