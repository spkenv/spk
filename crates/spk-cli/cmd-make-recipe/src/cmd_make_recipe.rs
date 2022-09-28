// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Args;
use spk_cli_common::{flags, CommandArgs, Run};
use spk_schema::foundation::format::FormatOptionMap;
use spk_schema::foundation::spec_ops::Named;
use spk_schema::{SpecTemplate, Template, TemplateExt};

/// Render a package spec template into a recipe
///
/// This is done automatically when building packages, but can
/// be a useful debugging tool when writing package spec files.
#[derive(Args)]
#[clap(visible_aliases = &["mkrecipe", "mkr"])]
pub struct MakeRecipe {
    #[clap(flatten)]
    pub options: flags::Options,

    #[clap(short, long, global = true, parse(from_occurrences))]
    pub verbose: u32,

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
    async fn run(&mut self) -> Result<i32> {
        let options = self.options.get_options()?;

        let template = match flags::find_package_template(&self.package)? {
            flags::FindPackageTemplateResult::NotFound(name) => {
                Arc::new(SpecTemplate::from_file(name.as_ref())?)
            }
            res => {
                let (_, template) = res.must_be_found();
                template
            }
        };

        tracing::info!("rendering template for {}", template.name());
        tracing::info!("using options {}", options.format_option_map());
        let rendered = spk_schema_liquid::render_template(template.source(), &options)
            .context("Failed to render template")?;
        print!("{rendered}");

        match template.render(&options) {
            Err(err) => {
                tracing::error!("This template did not render into a valid spec {err}");
                Ok(1)
            }
            Ok(_) => {
                tracing::info!("Successfully rendered a valid spec");
                Ok(0)
            }
        }
    }
}
