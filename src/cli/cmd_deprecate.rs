// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::sync::Arc;

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
        let repos: Vec<_> = self
            .repos
            .get_repos(None)?
            .into_iter()
            .map(|(name, repo)| (name, Arc::new(repo)))
            .collect();
        if repos.is_empty() {
            eprintln!(
                "{}",
                "No repositories selected, specify --local-repo (-l) and/or --enable-repo (-r)"
                    .yellow()
            );
            return Ok(1);
        }

        // find and load everything that we want to deprecate first
        // to avoid doing some deprecations and then failing in the
        // middle of the operation. This is still not properly atomic
        // but avoids the simple failure cases
        let mut to_deprecate = Vec::new();
        for name in self.packages.iter() {
            if !name.contains('/') {
                tracing::error!("Must provide a version number: {name}/???");
                tracing::error!(" > use 'spk ls {name}' to view available versions");
                return Ok(1);
            }
            let ident = spk::api::parse_ident(name)?;
            for (repo_name, repo) in repos.iter() {
                let spec = repo.read_spec(&ident)?;
                to_deprecate.push((spec, repo_name.clone(), Arc::clone(repo)));
            }
        }

        for (mut spec, repo_name, repo) in to_deprecate.into_iter() {
            let fmt = spk::io::format_ident(&spec.pkg);
            if spec.deprecated {
                tracing::warn!(repo=%repo_name, "no change  {} (already deprecated)", fmt);
                continue;
            }
            spec.deprecated = true;
            repo.force_publish_spec(spec)?;
            tracing::info!(repo=%repo_name, "deprecated {}", fmt);
        }
        Ok(0)
    }
}
