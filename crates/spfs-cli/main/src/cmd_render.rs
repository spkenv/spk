// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use clap::Parser;

use spfs::{prelude::*, Error};
use spfs_cli_common as cli;

cli::main!(CmdRender);

#[derive(Debug, Parser)]
pub struct CmdRender {
    #[clap(flatten)]
    sync: cli::Sync,

    #[clap(short, long, parse(from_occurrences))]
    verbose: usize,

    /// Allow re-rendering when the target directory is not empty
    #[clap(long = "allow-existing")]
    allow_existing: bool,

    /// The tag or digest of what to render, use a '+' to join multiple layers
    reference: String,

    /// Alternate path to render the manifest into (defaults to the local repository)
    target: Option<std::path::PathBuf>,
}

impl CmdRender {
    pub async fn run(&mut self, config: &spfs::Config) -> spfs::Result<i32> {
        let env_spec = spfs::tracking::EnvSpec::parse(&self.reference)?;
        let (repo, origin) = tokio::try_join!(
            config.get_local_repository_handle(),
            config.get_remote("origin")
        )?;

        let synced = self
            .sync
            .get_syncer(&origin, &repo)
            .sync_env(env_spec)
            .await?;

        let path = match &self.target {
            Some(target) => self.render_to_dir(synced.env, target).await?,
            None => self.render_to_repo(synced.env, config).await?,
        };

        tracing::info!("render completed successfully");
        println!("{}", path.display());
        Ok(0)
    }

    async fn render_to_dir(
        &self,
        env_spec: spfs::tracking::EnvSpec,
        target: &std::path::Path,
    ) -> spfs::Result<std::path::PathBuf> {
        tokio::fs::create_dir_all(&target)
            .await
            .map_err(|err| Error::RuntimeWriteError(target.to_owned(), err))?;
        let target_dir = tokio::fs::canonicalize(target)
            .await
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
            return Err(format!("Directory is not empty {}", target_dir.display()).into());
        }
        tracing::info!("rendering into {}", target_dir.display());
        spfs::render_into_directory(&env_spec, &target_dir).await?;
        Ok(target_dir)
    }

    async fn render_to_repo(
        &self,
        env_spec: spfs::tracking::EnvSpec,
        config: &spfs::Config,
    ) -> spfs::Result<std::path::PathBuf> {
        let repo = config.get_local_repository().await?;
        let renders = repo.renders()?;
        let mut digests = Vec::with_capacity(env_spec.len());
        for env_item in env_spec.iter() {
            let env_item = env_item.to_string();
            let digest = repo.resolve_ref(env_item.as_ref()).await?;
            digests.push(digest);
        }

        let handle = repo.into();
        let layers = spfs::resolve_stack_to_layers(digests.iter(), Some(&handle)).await?;
        let mut manifests = Vec::with_capacity(layers.len());
        for layer in layers {
            manifests.push(handle.read_manifest(layer.manifest).await?);
        }
        if manifests.len() > 1 {
            tracing::info!("merging {} layers into one", manifests.len())
        }
        let merged = manifests.into_iter().map(|m| m.unlock()).fold(
            spfs::tracking::Manifest::default(),
            |mut acc, m| {
                acc.update(&m);
                acc
            },
        );
        renders
            .render_manifest(&spfs::graph::Manifest::from(&merged))
            .await
    }
}
