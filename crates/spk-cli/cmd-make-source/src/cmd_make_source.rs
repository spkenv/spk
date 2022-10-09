// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Args;
use spk_build::SourcePackageBuilder;
use spk_cli_common::{flags, CommandArgs, Run};
use spk_schema::foundation::format::FormatIdent;
use spk_schema::foundation::spec_ops::Named;
use spk_schema::ident::LocatedBuildIdent;
use spk_schema::{Package, Recipe, SpecTemplate, Template, TemplateExt};
use spk_storage::{self as storage};

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
    pub async fn make_source(&mut self) -> Result<Vec<LocatedBuildIdent>> {
        let _runtime = self
            .runtime
            .ensure_active_runtime(&["make-source", "mksource", "mksrc", "mks"])
            .await?;
        let local: storage::RepositoryHandle = storage::local_repository().await?.into();
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
                    Arc::new(SpecTemplate::from_file(name.as_ref())?)
                }
                res => {
                    let (_, template) = res.must_be_found();
                    template
                }
            };
            let root = template
                .file_path()
                .parent()
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

            tracing::info!("rendering template for {}", template.name());
            let recipe = template.render(&options)?;
            let ident = recipe.to_ident();

            tracing::info!("saving package recipe for {}", ident.format_ident());
            local.force_publish_recipe(&recipe).await?;

            tracing::info!("collecting sources for {}", ident.format_ident());
            let (out, _components) = SourcePackageBuilder::from_recipe(recipe)
                .build_and_publish(root, &local)
                .await
                .context("Failed to collect sources")?;
            tracing::info!("created {}", out.ident().format_ident());
            idents.push(
                out.ident()
                    .clone()
                    .try_into_located_build_ident(local.name().to_owned())?,
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
