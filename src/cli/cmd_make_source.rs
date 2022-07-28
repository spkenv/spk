// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Args;
use spk::io::Format;
use spk::prelude::*;

use super::{flags, CommandArgs, Run};

/// Build a source package from a spec file.
#[derive(Args)]
#[clap(visible_aliases = &["mksource", "mksrc", "mks"])]
pub struct MakeSource {
    #[clap(flatten)]
    pub runtime: flags::Runtime,

    #[clap(flatten)]
    pub options: flags::Options,

    #[clap(short, long, global = true, parse(from_occurrences))]
    pub verbose: u32,

    /// The packages or yaml spec files to collect
    #[clap(name = "PKG|SPEC_FILE")]
    pub packages: Vec<String>,
}

#[async_trait::async_trait]
impl Run for MakeSource {
    async fn run(&mut self) -> Result<i32> {
        self.make_source().await.map(|_| 0)
    }
}

impl MakeSource {
    pub(crate) async fn make_source(&mut self) -> Result<Vec<spk::api::BuildIdent>> {
        let _runtime = self.runtime.ensure_active_runtime().await?;
        let local: spk::storage::RepositoryHandle = spk::storage::local_repository().await?.into();
        let options = self.options.get_options()?;

        let mut packages: Vec<_> = self.packages.iter().cloned().map(Some).collect();
        if packages.is_empty() {
            packages.push(None)
        }

        let mut idents = Vec::new();

        for package in packages.into_iter() {
            let template = match flags::find_package_template(&package)? {
                flags::FindPackageTemplateResult::NotFound(name) => {
                    // TODO:: load from given repos
                    Arc::new(spk::api::SpecTemplate::from_file(name.as_ref())?)
                }
                res => {
                    let (_, template) = res.must_be_found();
                    template
                }
            };

            tracing::info!("rendering template for {}", template.name());
            let recipe = template.render(&options)?;
            let ident = recipe.ident();

            tracing::info!("saving package recipe for {}", ident.format_ident());
            local.force_publish_recipe(&recipe).await?;

            tracing::info!("collecting sources for {}", ident.format_ident());
            let (out, _components) = spk::build::SourcePackageBuilder::from_recipe(recipe)
                .build_and_publish(&local)
                .await
                .context("Failed to collect sources")?;
            tracing::info!("created {}", out.ident().format_ident());
            idents.push(
                out.ident()
                    .clone()
                    .try_into_build_ident(local.name().to_owned())?,
            );
        }
        Ok(idents)
    }
}

impl CommandArgs for MakeSource {
    fn get_positional_args(&self) -> Vec<String> {
        // The important positional args for a make-source are the packages
        self.packages.clone()
    }
}
