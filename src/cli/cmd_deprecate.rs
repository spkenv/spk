// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use anyhow::Result;
use clap::Args;
use colored::Colorize;

use super::flags;

/// Deprecate a package in a repository.
///
// Deprecated packages can still be resolved by requesting the exact build,
// but will otherwise not show up in environments. By deprecating a package
// version, as opposed to an individual build, the package will also no longer
// be rebuilt from source under any circumstances. This also deprecates all builds
// by association.
#[derive(Args, Clone)]
pub struct Deprecate {
    #[clap(flatten)]
    repos: flags::Repositories,

    /// The package version or build to deprecate
    ///
    /// By deprecating a package version, as opposed to an individual
    /// build, the package will also no longer be rebuilt from source
    /// under any circumstances. This also deprecates all builds by
    /// association.
    #[clap(name = "PKG", required = true)]
    packages: Vec<String>,
}

/// Runs make-source and then make-binary
impl Deprecate {
    pub fn run(&self) -> Result<i32> {
        let repos = self.repos.get_repos(None)?;
        if repos.is_empty() {
            eprintln!(
                "{}",
                "No repositories selected, specify --local-repo (-l) and/or --enable-repo (-r)"
                    .yellow()
            );
            return Ok(1);
        }

        for name in self.packages.iter() {
            if !name.contains('/') {
                tracing::error!("Must provide a version number: {name}/???");
                tracing::error!(" > use 'spk ls {name}' to view available versions");
                return Ok(1);
            }
            let ident = spk::api::parse_ident(name)?;
            for (repo_name, repo) in repos.iter() {
                let mut spec = repo.read_spec(&ident)?;
                spec.deprecated = true;
                repo.force_publish_spec(spec)?;
                tracing::info!(repo=%repo_name, "deprecated {}", spk::io::format_ident(&ident));
            }
        }
        Ok(0)
    }
}
