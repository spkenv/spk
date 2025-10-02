// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use clap::Args;
use miette::{Context, Result};
use spk_cli_common::{CommandArgs, Run, flags};
use spk_schema::foundation::format::FormatOptionMap;
use spk_schema::{SpecFileData, Template};

/// Render a package spec template into a recipe
///
/// This is done automatically when building packages, but can
/// be a useful debugging tool when writing package spec files.
#[derive(Args)]
#[clap(visible_aliases = &["mkrecipe", "mkr"])]
pub struct MakeRecipe {
    #[clap(flatten)]
    pub options: flags::Options,

    #[clap(flatten)]
    pub workspace: flags::Workspace,

    #[clap(short, long, global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// The package spec file to render
    #[clap(name = "SPEC_FILE")]
    pub package: Option<String>,
}

impl CommandArgs for MakeRecipe {
    // The important positional arg for a make-recipe is the package
    fn get_positional_args(&self) -> Vec<String> {
        match self.package.clone() {
            Some(p) => vec![p],
            None => vec![],
        }
    }
}

#[async_trait::async_trait]
impl Run for MakeRecipe {
    type Output = i32;

    async fn run(&mut self) -> Result<Self::Output> {
        let options = self.options.get_options()?;
        let mut workspace = self.workspace.load_or_default()?;

        let template = match self.package.as_ref() {
            Some(p) => workspace.find_or_load_package_template(p),
            None => workspace.default_package_template().map_err(From::from),
        }
        .wrap_err("did not find recipe template")?;

        if let Some(name) = template.name() {
            tracing::info!("rendering template for {name}");
        } else {
            tracing::info!("rendering template without a name");
        }
        tracing::info!("using options {}", options.format_option_map());
        let rendered = template
            .render_to_string(&options)
            .wrap_err("Failed to render template")?;
        print!("{rendered}");
        match template.render(&options) {
            Err(err) => {
                tracing::error!("This template did not render into a valid spec {err}");
                Ok(1)
            }
            Ok(SpecFileData::Recipe(_)) => {
                tracing::info!("Successfully rendered a valid spec");
                Ok(0)
            }
            Ok(SpecFileData::Requests(_)) => {
                tracing::error!(
                    "This template did not render into a valid recipe spec. It is a requests spec"
                );
                Ok(2)
            }
        }
    }
}
