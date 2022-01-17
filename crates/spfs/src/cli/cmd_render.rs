// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use structopt::StructOpt;

use spfs::prelude::*;

#[macro_use]
mod args;

main!(CmdRender);

#[derive(Debug, StructOpt)]
pub struct CmdRender {
    #[structopt(short = "v", long = "verbose", parse(from_occurrences))]
    pub verbose: usize,
    #[structopt(
        long = "allow-existing",
        help = "Allow re-rendering when the target directory is not empty"
    )]
    allow_existing: bool,
    #[structopt(help = "The tag or digest of what to render, use a '+' to join multiple layers")]
    reference: String,
    #[structopt(
        help = "Alternate path to render the manifest into (defaults to the local repository)"
    )]
    target: Option<std::path::PathBuf>,
}

impl CmdRender {
    pub async fn run(&mut self, config: &spfs::Config) -> spfs::Result<i32> {
        let env_spec = spfs::tracking::EnvSpec::new(&self.reference)?;
        let repo = config.get_repository()?;

        for target in &env_spec.items {
            let target = target.to_string();
            if !repo.has_ref(target.as_str()).await {
                tracing::info!(reference = ?target, "pulling target ref");
                spfs::pull_ref(target.as_str()).await?;
            }
        }

        let path = match &self.target {
            Some(target) => self.render_to_dir(env_spec, target).await?,
            None => self.render_to_repo(env_spec, config).await?,
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
        std::fs::create_dir_all(&target)?;
        let target_dir = target.canonicalize()?;
        if std::fs::read_dir(&target_dir)?.next().is_some() && !self.allow_existing {
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
        let repo = config.get_repository()?;
        let renders = repo.renders()?;
        let mut digests = Vec::with_capacity(env_spec.items.len());
        for env_item in env_spec.items {
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
