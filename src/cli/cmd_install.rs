// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::{collections::HashSet, io::Write};

use anyhow::{Context, Result};
use clap::Args;
use colored::Colorize;

use super::{flags, Run};

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

    /// The packages to install
    #[clap(name = "PKG", required = true)]
    pub packages: Vec<String>,
}

impl Run for Install {
    fn run(&mut self) -> Result<i32> {
        let mut solver = self.solver.get_solver(&self.options)?;
        let requests = self
            .requests
            .parse_requests(&self.packages, &self.options)?;

        let env = spk::current_env()?;

        for solved in env.items() {
            solver.add_request(solved.request.into());
        }
        for request in requests {
            solver.add_request(request);
        }

        let solution = spk::io::run_and_print_resolve(&solver, self.verbose)?;

        println!("The following packages will be installed:\n");
        let requested: HashSet<_> = solver
            .get_initial_state()
            .pkg_requests
            .iter()
            .map(|r| r.pkg.name().to_string())
            .collect();
        let mut primary = Vec::new();
        let mut tertiary = Vec::new();
        for solved in solution.items() {
            if requested.contains(solved.spec.pkg.name()) {
                primary.push(solved.spec);
                continue;
            }
            if solution.get(solved.request.pkg.name()).is_none() {
                tertiary.push(solved.spec)
            }
        }

        println!("  Requested:");
        for spec in primary {
            let mut end = String::new();
            if spec.pkg.build.is_none() {
                end = " [build from source]".magenta().to_string();
            }
            println!("    {}{end}", spk::io::format_ident(&spec.pkg));
        }
        if !tertiary.is_empty() {
            println!("\n  Dependencies:");
        }
        for spec in tertiary {
            let mut end = String::new();
            if spec.pkg.build.is_none() {
                end = " [build from source]".magenta().to_string();
            }
            println!("    {}{end}", spk::io::format_ident(&spec.pkg))
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

        let compiled_solution = spk::build_required_packages(&solution)
            .context("Failed to build one or more packages from source")?;
        spk::setup_current_runtime(&compiled_solution)?;
        Ok(0)
    }
}
