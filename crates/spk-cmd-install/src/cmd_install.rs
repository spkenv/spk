// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::{collections::HashSet, io::Write};

use anyhow::{Context, Result};
use clap::Args;
use colored::Colorize;
use futures::TryFutureExt;
use spk_cli_common::{build_required_packages, current_env, flags, CommandArgs, Run};
use spk_exec::setup_current_runtime;
use spk_format::FormatIdent;
use spk_spec_ops::{Named, PackageOps};

/// Install a package into the current environment
#[derive(Args)]
pub struct Install {
    #[clap(flatten)]
    pub solver: flags::Solver,
    #[clap(flatten)]
    pub options: flags::Options,
    #[clap(flatten)]
    pub requests: flags::Requests,

    #[clap(short, long, global = true, parse(from_occurrences))]
    pub verbose: u32,

    /// Do not prompt for confirmation, just continue
    #[clap(long, short)]
    yes: bool,

    #[clap(flatten)]
    pub formatter_settings: flags::DecisionFormatterSettings,

    /// The packages to install
    #[clap(name = "PKG", required = true)]
    pub packages: Vec<String>,
}

#[async_trait::async_trait]
impl Run for Install {
    async fn run(&mut self) -> Result<i32> {
        let (mut solver, requests, env) = tokio::try_join!(
            self.solver.get_solver(&self.options),
            self.requests.parse_requests(&self.packages, &self.options),
            current_env().map_err(|err| err.into())
        )?;

        for solved in env.items() {
            solver.add_request(solved.request.into());
        }
        for request in requests {
            solver.add_request(request);
        }

        let formatter = self.formatter_settings.get_formatter(self.verbose);
        let solution = formatter.run_and_print_resolve(&solver).await?;

        println!("The following packages will be installed:\n");
        let requested: HashSet<_> = solver
            .get_initial_state()
            .get_pkg_requests()
            .iter()
            .map(|r| r.pkg.name.clone())
            .collect();
        let mut primary = Vec::new();
        let mut tertiary = Vec::new();
        for solved in solution.items() {
            if requested.contains(solved.spec.name()) {
                primary.push(solved.spec);
                continue;
            }
            if solution.get(&solved.request.pkg.name).is_none() {
                tertiary.push(solved.spec)
            }
        }

        println!("  Requested:");
        for spec in primary {
            let mut end = String::new();
            if spec.ident().build.is_none() {
                end = " [build from source]".magenta().to_string();
            }
            println!("    {}{end}", spec.ident().format_ident());
        }
        if !tertiary.is_empty() {
            println!("\n  Dependencies:");
        }
        for spec in tertiary {
            let mut end = String::new();
            if spec.ident().build.is_none() {
                end = " [build from source]".magenta().to_string();
            }
            println!("    {}{end}", spec.ident().format_ident())
        }

        println!();

        if !self.yes {
            let mut input = String::new();
            print!("Do you want to continue? [y/N]: ");
            let _ = std::io::stdout().flush();
            std::io::stdin().read_line(&mut input)?;
            match input.trim() {
                "y" | "yes" => {}
                _ => {
                    println!("Installation cancelled");
                    return Ok(1);
                }
            }
        }

        let compiled_solution = build_required_packages(&solution)
            .await
            .context("Failed to build one or more packages from source")?;
        setup_current_runtime(&compiled_solution).await?;
        Ok(0)
    }
}

impl CommandArgs for Install {
    fn get_positional_args(&self) -> Vec<String> {
        // The important positional args for an install are the packages
        self.packages.clone()
    }
}
