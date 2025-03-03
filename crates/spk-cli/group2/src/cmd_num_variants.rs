// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::sync::Arc;

use clap::Args;
use miette::{Result, WrapErr};
use spk_cli_common::{CommandArgs, Run, flags};
use spk_schema::Recipe;

/// Build a source package from a spec file.
#[derive(Args)]
pub struct NumVariants {
    #[clap(flatten)]
    pub repos: flags::Repositories,
    #[clap(flatten)]
    pub options: flags::Options,
    #[clap(flatten)]
    pub workspace: flags::Workspace,

    /// The local package yaml spec file or published packages/version to report on
    #[clap(name = "SPEC_FILE|PKG/VER")]
    pub package: Option<String>,
}

#[async_trait::async_trait]
impl Run for NumVariants {
    type Output = i32;

    async fn run(&mut self) -> Result<Self::Output> {
        let options = self.options.get_options()?;
        let names_and_repos = self.repos.get_repos_for_non_destructive_operation().await?;
        let repos = names_and_repos
            .into_iter()
            .map(|(_, r)| Arc::new(r))
            .collect::<Vec<_>>();
        let mut workspace = self.workspace.load_or_default()?;

        let (spec_data, filename) = flags::find_package_recipe_from_workspace_or_repo(
            self.package.as_ref(),
            &options,
            &mut workspace,
            &repos,
        )
        .await?;
        let recipe = spec_data.into_recipe().wrap_err_with(|| {
            format!(
                "{filename} was expected to contain a recipe",
                filename = filename.to_string_lossy()
            )
        })?;

        println!("{}", recipe.default_variants(&options).len());

        Ok(0)
    }
}

impl CommandArgs for NumVariants {
    fn get_positional_args(&self) -> Vec<String> {
        // The important positional arg for a num-variants is the package
        match &self.package {
            Some(pkg) => vec![pkg.clone()],
            None => vec![],
        }
    }
}
