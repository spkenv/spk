// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use clap::Args;
use miette::{bail, Context, Result};
use spk_build::SourcePackageBuilder;
use spk_cli_common::{flags, CommandArgs, Run};
use spk_schema::foundation::format::FormatIdent;
use spk_schema::foundation::spec_ops::Named;
use spk_schema::ident::LocatedBuildIdent;
use spk_schema::v0::Spec;
use spk_schema::{
    AnyIdent,
    Lint,
    LintedItem,
    Package,
    Recipe,
    SpecTemplate,
    Template,
    TemplateExt,
};
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

    /// The packages or yaml spec files to collect
    #[clap(name = "PKG|SPEC_FILE")]
    pub packages: Vec<String>,

    /// Populated with the created src to generate a summary from the caller.
    #[clap(skip)]
    pub created_src: Vec<String>,

    /// Used to gather lints to output at the end of a build.
    #[clap(skip)]
    pub lints: BTreeMap<String, Vec<Lint>>,
}

#[async_trait::async_trait]
impl Run for MakeSource {
    async fn run(&mut self) -> Result<i32> {
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
        let local: storage::RepositoryHandle = storage::local_repository().await?.into();
        let options = self.options.get_options()?;

        let mut packages: Vec<_> = self.packages.iter().cloned().map(Some).collect();
        if packages.is_empty() {
            packages.push(None)
        }

        let mut idents = Vec::new();

        for package in packages.into_iter() {
            let template = match flags::find_package_template(package.as_ref())? {
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
            let ident = recipe.ident();

            let lints: std::result::Result<LintedItem<Spec<AnyIdent>>, serde_yaml::Error> =
                serde_yaml::from_str(&template.render_to_string(&options)?);

            match lints {
                Ok(linted_item) => match linted_item.lints.is_empty() {
                    true => (),
                    false => {
                        self.lints
                            .insert(ident.format_ident(), linted_item.lints.clone());
                    }
                },
                Err(e) => tracing::error!("Failed to retrieve lints: {e}"),
            }

            tracing::info!("saving package recipe for {}", ident.format_ident());
            local.force_publish_recipe(&recipe).await?;

            tracing::info!("collecting sources for {}", ident.format_ident());
            let (out, _components) = SourcePackageBuilder::from_recipe(recipe)
                .build_and_publish(root, &local)
                .await
                .wrap_err("Failed to collect sources")?;
            tracing::info!("created {}", out.ident().format_ident());
            self.created_src
                .push(format!("   {}", out.ident().format_ident()));
            idents.push(out.ident().clone().into_located(local.name().to_owned()));
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
