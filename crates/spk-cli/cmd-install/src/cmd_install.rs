// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::collections::HashSet;
use std::io::Write;

use clap::Args;
use colored::Colorize;
use futures::TryFutureExt;
use miette::{Context, IntoDiagnostic, Result};
use spk_cli_common::{CommandArgs, Run, build_required_packages, current_env, flags};
use spk_exec::setup_current_runtime;
use spk_schema::Package;
use spk_schema::foundation::format::FormatIdent;
use spk_schema::foundation::spec_ops::Named;
use spk_solve::{Solver, SolverMut};

/// Install a package into the current environment
#[derive(Args)]
pub struct Install {
    #[clap(flatten)]
    pub solver: flags::Solver,
    #[clap(flatten)]
    pub options: flags::Options,
    #[clap(flatten)]
    pub requests: flags::Requests,

    #[clap(short, long, global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// Do not prompt for confirmation, just continue
    #[clap(long, short)]
    yes: bool,

    /// The packages to install
    #[clap(name = "PKG", required = true)]
    pub packages: Vec<String>,
}

#[async_trait::async_trait]
impl Run for Install {
    type Output = i32;

    async fn run(&mut self) -> Result<Self::Output> {
        let (mut solver, env) = tokio::try_join!(
            self.solver.get_solver(&self.options),
            current_env().map_err(|err| err.into())
        )?;

        let (requests, extra_options) = self
            .requests
            .parse_requests(&self.packages, &self.options, solver.repositories())
            .await?;
        solver.update_options(extra_options);
        for solved in env.items() {
            solver.add_request(solved.request.clone().into());
        }
        for request in requests {
            solver.add_request(request);
        }

        let formatter = self
            .solver
            .decision_formatter_settings
            .get_formatter(self.verbose)?;
        let solution = solver.run_and_print_resolve(&formatter).await?;

        println!("The following packages will be installed:\n");
        let requested: HashSet<_> = solver
            .get_pkg_requests()
            .iter()
            .map(|r| r.pkg.name.clone())
            .collect();
        let mut primary = Vec::new();
        let mut tertiary = Vec::new();
        for solved in solution.items() {
            if requested.contains(solved.spec.name()) {
                primary.push(solved);
                continue;
            }
            if solution.get(&solved.request.pkg.name).is_none() {
                tertiary.push(solved)
            }
        }

        println!("  Requested:");
        for resolved in primary {
            let mut end = String::new();
            if resolved.is_source_build() {
                end = " [build from source]".magenta().to_string();
            }
            println!("    {}{end}", resolved.spec.ident().format_ident());
        }
        if !tertiary.is_empty() {
            println!("\n  Dependencies:");
        }
        for resolved in tertiary {
            let mut end = String::new();
            if resolved.is_source_build() {
                end = " [build from source]".magenta().to_string();
            }
            println!("    {}{end}", resolved.spec.ident().format_ident())
        }

        println!();

        if !self.yes {
            let mut input = String::new();
            print!("Do you want to continue? [y/N]: ");
            let _ = std::io::stdout().flush();
            std::io::stdin().read_line(&mut input).into_diagnostic()?;
            match input.trim() {
                "y" | "yes" => {}
                _ => {
                    println!("Installation cancelled");
                    return Ok(1);
                }
            }
        }

        let compiled_solution = build_required_packages(&solution, solver)
            .await
            .wrap_err("Failed to build one or more packages from source")?;
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
