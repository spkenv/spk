// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Args;
use spk::api::Package;
use spk::io::Format;

use super::{flags, CommandArgs, Run};

/// Build a source package from a spec file.
#[derive(Args)]
#[clap(visible_aliases = &["mksource", "mksrc", "mks"])]
pub struct MakeSource {
    #[clap(flatten)]
    pub runtime: flags::Runtime,

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

        let mut packages: Vec<_> = self.packages.iter().cloned().map(Some).collect();
        if packages.is_empty() {
            packages.push(None)
        }

        let mut idents = Vec::new();

        for package in packages.into_iter() {
            let spec = match flags::find_package_spec(&package)? {
                flags::FindPackageSpecResult::NotFound(name) => {
                    // TODO:: load from given repos
                    Arc::new(spk::api::read_spec_file(name)?)
                }
                res => {
                    let (_, spec) = res.must_be_found();
                    tracing::info!("saving spec file {}", spec.ident().format_ident());
                    spk::save_spec(&spec).await?;
                    spec
                }
            };

            tracing::info!("collecting sources for {}", spec.ident().format_ident());
            let out = spk::build::SourcePackageBuilder::from_spec((*spec).clone())
                .build()
                .await
                .context("Failed to collect sources")?;
            tracing::info!("created {}", out.format_ident());
            idents.push(out)
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
