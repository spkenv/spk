// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use anyhow::{Context, Result};
use clap::Args;

use super::flags;

/// Build a source package from a spec file.
#[derive(Args)]
#[clap(visible_aliases = &["mksource", "mksrc", "mks"])]
pub struct MakeSource {
    #[clap(flatten)]
    pub runtime: flags::Runtime,

    #[clap(short, long, global = true, parse(from_occurrences))]
    pub verbose: u32,

    /// The packages or yaml spec files to collect
    #[clap(required = true, name = "PKG|SPEC_FILE")]
    pub packages: Vec<String>,
}

impl MakeSource {
    pub fn run(&self) -> Result<i32> {
        let _runtime = self.runtime.ensure_active_runtime()?;

        for package in self.packages.iter() {
            let spec = if std::path::Path::new(&package).is_file() {
                let spec = spk::api::read_spec_file(package)?;
                tracing::info!("saving spec file {}", spk::io::format_ident(&spec.pkg));
                spk::save_spec(spec.clone())?;
                spec
            } else {
                // TODO:: load from given repos
                spk::load_spec(package)?
            };

            tracing::info!(
                "collecting sources for {}",
                spk::io::format_ident(&spec.pkg)
            );
            let out = spk::build::SourcePackageBuilder::from_spec(spec)
                .build()
                .context("Failed to collect sources")?;
            tracing::info!("created {}", spk::io::format_ident(&out));
        }
        Ok(0)
    }
}
