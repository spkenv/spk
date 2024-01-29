// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

#[cfg(unix)]
use std::os::unix::process::ExitStatusExt;
#[cfg(windows)]
use std::os::windows::process::ExitStatusExt;
use std::process::ExitStatus;
use std::sync::Arc;

use clap::Args;
use miette::{Context, IntoDiagnostic, Result};
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
    type Output = ExitStatus;

    async fn run(&mut self) -> Result<Self::Output> {
        let options = self.options.get_options()?;

        let template = match flags::find_package_template(self.package.as_ref())? {
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
        let data = spk_schema::TemplateData::new(&options);
        tracing::debug!("full template data: {data:#?}");
        let rendered = spk_schema_liquid::render_template(template.source(), &data)
            .into_diagnostic()
            .wrap_err("Failed to render template")?;
        print!("{rendered}");

        match template.render(&options) {
            Err(err) => {
                tracing::error!("This template did not render into a valid spec {err}");
                Ok(ExitStatus::from_raw(1))
            }
            Ok(_) => {
                tracing::info!("Successfully rendered a valid spec");
                Ok(ExitStatus::default())
            }
        }
    }
}
