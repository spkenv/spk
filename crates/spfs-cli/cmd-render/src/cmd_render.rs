// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use clap::builder::TypedValueParser;
use clap::Parser;
use miette::{Context, Result};
use spfs::prelude::*;
use spfs::storage::fallback::FallbackProxy;
use spfs::{graph, Error, RenderResult};
use spfs_cli_common as cli;
use spfs_cli_common::CommandName;
use strum::VariantNames;

cli::main!(CmdRender);

#[derive(Debug, Parser)]
pub struct CmdRender {
    #[clap(flatten)]
    sync: cli::Sync,
    #[clap(flatten)]
    render: cli::Render,
    #[clap(flatten)]
    logging: cli::Logging,

    /// Allow re-rendering when the target directory is not empty
    #[clap(long = "allow-existing")]
    allow_existing: bool,

    /// The strategy to use when rendering. Defaults to `Copy` when
    /// using a local directory and `HardLink` for the repository.
    #[clap(
        long,
        value_parser = clap::builder::PossibleValuesParser::new(spfs::storage::fs::RenderType::VARIANTS)
            .map(|s| s.parse::<spfs::storage::fs::RenderType>().unwrap())
    )]
    strategy: Option<spfs::storage::fs::RenderType>,

    /// The tag or digest of what to render, use a '+' to join multiple layers
    reference: String,

    /// Alternate path to render the manifest into (defaults to the local repository)
    target: Option<std::path::PathBuf>,
}

impl CommandName for CmdRender {
    fn command_name(&self) -> &'static str {
        "render"
    }
}

impl CmdRender {
    pub async fn run(&mut self, config: &spfs::Config) -> Result<i32> {
        let env_spec = spfs::tracking::EnvSpec::parse(&self.reference)?;
        let (repo, origin, remotes) = tokio::try_join!(
            config.get_opened_local_repository(),
            config.get_remote("origin"),
            config.list_remotes()
        )?;

        let handle = repo.clone().into();
        let synced = self
            .sync
            .get_syncer(&origin, &handle)
            .sync_env(env_spec)
            .await?;

        // Use PayloadFallback to repair any missing payloads found in the
        // local repository by copying from any of the configure remotes.
        let fallback = FallbackProxy::new(repo, remotes);

        let rendered = match &self.target {
            Some(target) => self.render_to_dir(fallback, synced.env, target).await?,
            None => self.render_to_repo(fallback, synced.env).await?,
        };

        tracing::debug!("render(s) completed successfully");
        println!("{}", serde_json::json!(rendered));

        Ok(0)
    }

    async fn render_to_dir(
        &self,
        repo: FallbackProxy,
        env_spec: spfs::tracking::EnvSpec,
        target: &std::path::Path,
    ) -> Result<RenderResult> {
        tokio::fs::create_dir_all(&target)
            .await
            .map_err(|err| Error::RuntimeWriteError(target.to_owned(), err))?;
        let target_dir = tokio::task::block_in_place(|| dunce::canonicalize(target))
            .map_err(|err| Error::InvalidPath(target.to_owned(), err))?;
        if tokio::fs::read_dir(&target_dir)
            .await
            .map_err(|err| Error::RuntimeReadError(target_dir.clone(), err))?
            .next_entry()
            .await
            .map_err(|err| Error::RuntimeReadError(target_dir.clone(), err))?
            .is_some()
            && !self.allow_existing
        {
            miette::bail!("Directory is not empty {}", target_dir.display());
        }
        tracing::info!("rendering into {}", target_dir.display());

        let console_render_reporter = spfs::storage::fs::ConsoleRenderReporter::default();
        let render_summary_reporter = spfs::storage::fs::RenderSummaryReporter::default();

        let renderer = self.render.get_renderer(
            &repo,
            spfs::storage::fs::MultiReporter::new([
                &console_render_reporter as &dyn spfs::storage::fs::RenderReporter,
                &render_summary_reporter as &dyn spfs::storage::fs::RenderReporter,
            ]),
        );
        renderer
            .render_into_directory(
                env_spec,
                &target_dir,
                self.strategy.unwrap_or(spfs::storage::fs::RenderType::Copy),
            )
            .await?;
        Ok(RenderResult {
            paths_rendered: vec![target_dir],
            render_summary: render_summary_reporter.into_summary(),
        })
    }

    async fn render_to_repo(
        &self,
        repo: FallbackProxy,
        env_spec: spfs::tracking::EnvSpec,
    ) -> Result<RenderResult> {
        let mut stack = graph::Stack::default();
        for env_item in env_spec.iter() {
            let env_item = env_item.to_string();
            let digest = repo
                .resolve_ref(env_item.as_ref())
                .await
                .wrap_err_with(|| format!("resolve ref '{env_item}'"))?;
            stack.push(digest);
        }

        let layers = spfs::resolve_stack_to_layers_with_repo(&stack, &repo)
            .await
            .wrap_err("resolve stack to layers")?;

        let console_render_reporter = spfs::storage::fs::ConsoleRenderReporter::default();
        let render_summary_reporter = spfs::storage::fs::RenderSummaryReporter::default();

        let renderer = self.render.get_renderer(
            &repo,
            spfs::storage::fs::MultiReporter::new([
                &console_render_reporter as &dyn spfs::storage::fs::RenderReporter,
                &render_summary_reporter as &dyn spfs::storage::fs::RenderReporter,
            ]),
        );
        let stack = layers
            .into_iter()
            .filter_map(|l| l.manifest().copied())
            .collect();
        tracing::trace!("stack: {:?}", stack);
        renderer
            .render(&stack, self.strategy)
            .await
            .map(|paths_rendered| RenderResult {
                paths_rendered,
                render_summary: render_summary_reporter.into_summary(),
            })
            .map_err(Into::<miette::Error>::into)
            .wrap_err("render layers")
    }
}
