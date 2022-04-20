// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Args;

use super::flags;

/// Build a binary package from a spec file or source package.
#[derive(Args)]
#[clap(visible_aliases = &["mkbinary", "mkbin", "mkb"])]
pub struct MakeBinary {
    #[clap(flatten)]
    pub repos: flags::Repositories,
    #[clap(flatten)]
    pub options: flags::Options,
    #[clap(flatten)]
    pub runtime: flags::Runtime,

    #[clap(short, long, global = true, parse(from_occurrences))]
    pub verbose: u32,

    /// Build from the current directory, instead of a source package)
    #[clap(long)]
    pub here: bool,

    /// Setup the build, but instead of running the build script start an interactive shell
    #[clap(long, short)]
    pub interactive: bool,

    /// Build the first variant of this package, and then immediately enter a shell environment with it
    #[clap(long, short)]
    pub env: bool,

    /// The packages or yaml spec files to build
    #[clap(name = "PKG|SPEC_FILE")]
    pub packages: Vec<String>,
}

impl MakeBinary {
    pub fn run(&self) -> Result<i32> {
        let _runtime = self.runtime.ensure_active_runtime()?;

        let options = self.options.get_options()?;
        let repos: Vec<_> = self
            .repos
            .get_repos(&["origin".to_string()])?
            .into_iter()
            .map(|(_, r)| Arc::new(r))
            .collect();

        let mut packages: Vec<_> = self.packages.iter().cloned().map(Some).collect();
        if packages.is_empty() {
            packages.push(None)
        }

        for package in packages {
            let spec = match flags::find_package_spec(package)? {
                flags::FindPackageSpecResult::NotFound(name) => {
                    // TODO:: load from given repos
                    spk::api::read_spec_file(name)?
                }
                res => {
                    let (_, spec) = res.must_be_found();
                    tracing::info!("saving spec file {}", spk::io::format_ident(&spec.pkg));
                    spk::save_spec(spec.clone())?;
                    spec
                }
            };

            tracing::info!(
                "building binary package {}",
                spk::io::format_ident(&spec.pkg)
            );
            let mut built = std::collections::HashSet::new();
            for variant in spec.build.variants.iter() {
                let mut opts = if !self.options.no_host {
                    spk::api::host_options()?
                } else {
                    spk::api::OptionMap::default()
                };

                opts.extend(variant.clone());
                opts.extend(options.clone());
                let digest = opts.digest_str();
                if !built.insert(digest) {
                    continue;
                }

                tracing::info!("building variant {}", spk::io::format_options(&opts));
                let mut builder = spk::build::BinaryPackageBuilder::from_spec(spec.clone());
                let verbose = self.verbose;
                builder
                    .with_options(opts.clone())
                    .with_repositories(repos.iter().cloned())
                    .set_interactive(self.interactive)
                    .watch_source_resolve(move |r| spk::io::run_and_print_decisions(r, verbose))
                    .watch_build_resolve(move |r| spk::io::run_and_print_decisions(r, verbose));
                if self.here {
                    let here =
                        std::env::current_dir().context("Failed to get current directory")?;
                    builder.with_source(spk::build::BuildSource::LocalPath(here));
                }
                let out = match builder.build() {
                    Err(err @ spk::Error::Solve(_))
                    | Err(err @ spk::Error::PackageNotFoundError(_)) => {
                        tracing::error!("variant failed {}", spk::io::format_options(&opts));
                        return Err(err.into());
                    }
                    Ok(out) => out,
                    Err(err) => return Err(err.into()),
                };
                tracing::info!("created {}", spk::io::format_ident(&out.pkg));

                if self.env {
                    let request = spk::api::PkgRequest::from_ident(&out.pkg);
                    let mut cmd = std::process::Command::new("spk");
                    cmd.args(&["env", "--local-repo"])
                        .arg(request.pkg.to_string());
                    tracing::info!("entering environment with new package...");
                    tracing::debug!("{:?}", cmd);
                    let status = cmd.status()?;
                    return Ok(status.code().unwrap_or(1));
                }
            }
        }
        Ok(0)
    }
}
