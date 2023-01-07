// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::sync::Arc;

use anyhow::Result;
use clap::Args;
use spk_cli_common::{flags, CommandArgs, Run};
use spk_schema::Recipe;

/// Build a source package from a spec file.
#[derive(Args)]
pub struct NumVariants {
    #[clap(flatten)]
    pub repos: flags::Repositories,
    #[clap(flatten)]
    pub options: flags::Options,

    /// The local package yaml spec file or published packages/version to report on
    #[clap(name = "SPEC_FILE|PKG/VER")]
    pub package: Option<String>,
}

#[async_trait::async_trait]
impl Run for NumVariants {
    async fn run(&mut self) -> Result<i32> {
        let options = self.options.get_options()?;
        let names_and_repos = self.repos.get_repos_for_non_destructive_operation().await?;
        let repos = names_and_repos
            .into_iter()
            .map(|(_, r)| Arc::new(r))
            .collect::<Vec<_>>();

        let (recipe, _) =
            flags::find_package_recipe_from_template_or_repo(&self.package, &options, &repos)
                .await?;

        println!("{}", recipe.default_variants().len());

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
