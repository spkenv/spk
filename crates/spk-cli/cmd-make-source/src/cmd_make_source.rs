// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::sync::Arc;

use clap::Args;
use miette::{bail, Context, Result};
use spk_build::SourcePackageBuilder;
use spk_cli_common::{flags, BuildArtifact, BuildResult, CommandArgs, Run};
use spk_schema::foundation::format::FormatIdent;
use spk_schema::ident::LocatedBuildIdent;
use spk_schema::{Package, Recipe};
use spk_storage as storage;

/// Build a source package from a spec file.
#[derive(Args)]
#[clap(visible_aliases = &["mksource", "mksrc", "mks"])]
pub struct MakeSource {
    #[clap(flatten)]
    pub runtime: flags::Runtime,

    #[clap(flatten)]
    pub options: flags::Options,

    #[clap(short, long, global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,

    #[clap(flatten)]
    pub packages: flags::Packages,

    /// Populated with the created src to generate a summary from the caller.
    #[clap(skip)]
    pub created_src: BuildResult,
}

#[async_trait::async_trait]
impl Run for MakeSource {
    type Output = i32;

    async fn run(&mut self) -> Result<Self::Output> {
        self.make_source().await.map(|_| 0)
    }
}

impl MakeSource {
    pub async fn make_source(&mut self) -> Result<Vec<LocatedBuildIdent>> {
        if spfs::get_config()?
            .storage
            .allow_payload_sharing_between_users
        {
            bail!(
                "Building packages disabled when 'allow_payload_sharing_between_users' is enabled"
            );
        }

        let _runtime = self
            .runtime
            .ensure_active_runtime(&["make-source", "mksource", "mksrc", "mks"])
            .await?;
        let local = Arc::new(storage::local_repository().await?.into());
        let options = self.options.get_options()?;

        let mut idents = Vec::new();

        for (_package, spec_data, path) in self
            .packages
            .find_all_recipes(&options, &[Arc::clone(&local)])
            .await?
        {
            let root = path.parent().unwrap_or_else(|| std::path::Path::new("."));
            let recipe = spec_data.into_recipe()?;
            let ident = recipe.ident();

            tracing::info!("saving package recipe for {}", ident.format_ident());
            local.force_publish_recipe(&recipe).await?;

            tracing::info!("collecting sources for {}", ident.format_ident());
            let (out, _components) =
                SourcePackageBuilder::from_recipe(Arc::unwrap_or_clone(recipe))
                    .build_and_publish(root, &*local)
                    .await
                    .wrap_err("Failed to collect sources")?;
            tracing::info!("created {}", out.ident().format_ident());
            self.created_src.push(
                path.display().to_string(),
                BuildArtifact::Source(out.ident().clone()),
            );
            idents.push(out.ident().clone().into_located(local.name().to_owned()));
        }
        Ok(idents)
    }
}

impl CommandArgs for MakeSource {
    fn get_positional_args(&self) -> Vec<String> {
        // The important positional args for a make-source are the packages
        self.packages.get_positional_args()
    }
}
