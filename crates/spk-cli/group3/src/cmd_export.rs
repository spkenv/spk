// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::sync::Arc;

use clap::{Args, ValueHint};
use colored::Colorize;
use miette::{Result, bail};
use spk_cli_common::{CommandArgs, Run, build_required_packages, flags};
use spk_schema::Package;
use spk_solve::{Solver, SolverMut};
use spk_storage as storage;

#[cfg(test)]
#[path = "./cmd_export_test.rs"]
mod cmd_export_test;

/// Export a package as a tar file
#[derive(Args)]
pub struct Export {
    #[clap(flatten)]
    pub solver: flags::Solver,
    #[clap(flatten)]
    pub options: flags::Options,
    #[clap(flatten)]
    pub requests: flags::Requests,

    #[clap(short, long, global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// Resolve one or more requests and export all solved packages
    ///
    /// Because this flag accepts a variable number of requests, the output
    /// path cannot be given as a positional argument in this mode; use
    /// --file/-f to specify a custom output filename.
    #[clap(
        long = "env",
        value_name = "REQUEST",
        num_args = 1..,
        action = clap::ArgAction::Append
    )]
    pub env_requests: Vec<String>,

    /// The package to export (single-package mode)
    #[clap(name = "PKG", required_unless_present = "env_requests")]
    pub package: Option<String>,

    // The output path can be given two ways: as a positional FILE (the
    // long-standing single-package spelling) or via --file/-f. clap cannot
    // back both spellings with a single field, since a positional argument
    // cannot also carry a long/short flag, so each spelling needs its own
    // field. --env mode is variadic and would swallow a trailing positional,
    // so only --file works there. The two are resolved into one effective
    // path in `run`, where supplying both at once is rejected.
    /// The file to export into (single-package mode)
    ///
    /// A convenience spelling for single-package exports. The same path can
    /// be given with --file/-f instead; the two cannot be combined. This
    /// positional form is not available in --env mode.
    #[arg(value_hint = ValueHint::FilePath, value_name = "FILE")]
    pub positional_file: Option<std::path::PathBuf>,

    /// The file to export into
    ///
    /// Works in both single-package and --env mode. In single-package mode
    /// it is an alternative to the positional FILE. In --env mode it is the
    /// only way to set a custom output path, since the path cannot be given
    /// as a positional argument there; if omitted in --env mode a filename
    /// is derived from the requests.
    #[arg(
        long = "file",
        short = 'f',
        value_hint = ValueHint::FilePath,
        value_name = "FILE"
    )]
    pub output_file: Option<std::path::PathBuf>,
}

impl Export {
    fn get_source_spfs_repos_from_handles(
        repo_handles: &[Arc<storage::RepositoryHandle>],
    ) -> Result<Vec<&storage::SpfsRepository>> {
        repo_handles
            .iter()
            .map(|repo| match &**repo {
                storage::RepositoryHandle::SPFS(repo) => Ok(repo),
                storage::RepositoryHandle::Mem(_)
                | storage::RepositoryHandle::Runtime(_)
                | storage::RepositoryHandle::Indexed(_) => {
                    bail!("Only spfs repositories are supported")
                }
            })
            .collect::<Result<Vec<_>>>()
    }

    fn derive_single_package_filename(pkg: &spk_schema::AnyIdent) -> std::path::PathBuf {
        let mut build = String::new();
        if let Some(b) = pkg.build() {
            build = format!("_{b}");
        }
        std::path::PathBuf::from(format!("{}_{}{build}.spk", pkg.name(), pkg.version()))
    }

    fn sanitize_filename_base(input: &str) -> String {
        let mut out = String::new();
        let mut last_was_dash = false;
        for ch in input.chars() {
            let mapped = if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch.to_ascii_lowercase()
            } else {
                '-'
            };
            if mapped == '-' {
                if last_was_dash {
                    continue;
                }
                last_was_dash = true;
            } else {
                last_was_dash = false;
            }
            out.push(mapped);
        }
        let trimmed = out.trim_matches('-');
        if trimmed.is_empty() {
            "solution".to_string()
        } else {
            trimmed.to_string()
        }
    }

    fn derive_env_filename(requests: &[String]) -> std::path::PathBuf {
        let base = requests
            .first()
            .map(|req| Self::sanitize_filename_base(req))
            .unwrap_or_else(|| "solution".to_string());
        if requests.len() > 1 {
            std::path::PathBuf::from(format!("{base}-plus-{}.spk", requests.len() - 1))
        } else {
            std::path::PathBuf::from(format!("{base}.spk"))
        }
    }

    fn warn_and_cleanup_failed_archive(
        filename: &std::path::Path,
        res: &std::result::Result<(), spk_storage::Error>,
        warn_for_package_not_found: bool,
    ) {
        if warn_for_package_not_found && let Err(spk_storage::Error::PackageNotFound(_)) = res {
            tracing::warn!("Ensure that you are specifying at least a package and");
            tracing::warn!("version number when exporting from the local repository");
        }
        if res.is_err()
            && let Err(err) = std::fs::remove_file(filename)
        {
            tracing::warn!(?err, path=?filename, "failed to clean up incomplete archive");
        }
    }
}

#[async_trait::async_trait]
impl Run for Export {
    type Output = i32;

    async fn run(&mut self) -> Result<Self::Output> {
        if self.positional_file.is_some() && self.output_file.is_some() {
            bail!("Specify either a positional FILE or --file, not both");
        }
        if !self.env_requests.is_empty()
            && (self.package.is_some() || self.positional_file.is_some())
        {
            bail!("--env mode does not accept positional PKG/FILE arguments");
        }

        if self.env_requests.is_empty() {
            let options = self.options.get_options()?;
            let names_and_repos = self
                .solver
                .repos
                .get_repos_for_non_destructive_operation()
                .await?;
            let repo_handles = names_and_repos
                .into_iter()
                .map(|(_, r)| Arc::new(r))
                .collect::<Vec<_>>();
            let repos = Self::get_source_spfs_repos_from_handles(repo_handles.as_slice())?;

            let package = self
                .package
                .as_ref()
                .expect("clap should require PKG when --env is not provided");
            let pkg = self
                .requests
                .parse_idents(&options, [package.as_str()], repo_handles.as_slice())
                .await?
                .pop()
                .unwrap();

            let filename = self
                .output_file
                .clone()
                .or_else(|| self.positional_file.clone())
                .unwrap_or_else(|| Self::derive_single_package_filename(&pkg));
            let res = storage::export_package(repos.as_slice(), &pkg, &filename).await;
            Self::warn_and_cleanup_failed_archive(&filename, &res, true);
            res?;
            println!("{}: {:?}", "Created".green(), filename);
            return Ok(0);
        }

        let mut solver = self.solver.get_solver(&self.options).await?;
        let (requests, extra_options) = self
            .requests
            .parse_requests(&self.env_requests, &self.options, solver.repositories())
            .await?;
        solver.update_options(extra_options);
        for request in requests {
            solver.add_request(request);
        }

        let formatter = self
            .solver
            .decision_formatter_settings
            .get_formatter(self.verbose)?;
        let solution = solver.run_and_print_resolve(&formatter).await?;
        let compiled_solution = build_required_packages(&solution, solver.clone()).await?;

        let repo_handles = compiled_solution.repositories();
        let repos = Self::get_source_spfs_repos_from_handles(repo_handles.as_slice())?;
        let solved_packages = compiled_solution
            .items()
            .map(|item| item.spec.ident().to_any_ident())
            .collect::<std::collections::BTreeSet<_>>();

        let filename = self
            .output_file
            .clone()
            .unwrap_or_else(|| Self::derive_env_filename(&self.env_requests));
        let res =
            storage::export_packages(repos.as_slice(), solved_packages.into_iter(), &filename)
                .await;
        Self::warn_and_cleanup_failed_archive(&filename, &res, false);
        res?;
        println!("{}: {:?}", "Created".green(), filename);
        Ok(0)
    }
}

impl CommandArgs for Export {
    fn get_positional_args(&self) -> Vec<String> {
        // The important positional args for an export are the packages
        if !self.env_requests.is_empty() {
            return self.env_requests.clone();
        }
        self.package.clone().into_iter().collect()
    }
}
