// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use anyhow::{bail, Context, Result};
use clap::Args;
use colored::Colorize;

use super::{flags, Run};

/// View the current environment or information about a package
#[derive(Args)]
#[clap(visible_alias = "info")]
pub struct View {
    #[clap(flatten)]
    requests: super::flags::Requests,
    #[clap(flatten)]
    options: super::flags::Options,
    #[clap(flatten)]
    solver: super::flags::Solver,

    #[clap(short, long, global = true, parse(from_occurrences))]
    pub verbose: u32,

    /// The package to show information about
    package: Option<String>,

    /// Display information about the variants defined by the package
    #[clap(long)]
    variants: bool,
}

impl Run for View {
    fn run(&mut self) -> Result<i32> {
        if self.variants {
            return self.print_variants_info();
        }

        let package = match &self.package {
            None => return self.print_current_env(),
            Some(p) => p,
        };

        let mut solver = self.solver.get_solver(&self.options)?;
        let request = self.requests.parse_request(&package, &self.options)?;
        if self.verbose > spk::io::SHOW_INITIAL_REQUESTS_LEVEL {
            println!("{}", spk::io::format_initial_request(&request));
        }
        solver.add_request(request.clone());
        let request = match request {
            spk::api::Request::Pkg(pkg) => pkg,
            _ => bail!("Not a package request: {request:?}"),
        };

        let mut runtime = solver.run();
        let solution = match runtime.solution() {
            Ok(s) => s,
            Err(err @ spk::Error::Solve(_)) => {
                println!("{}", err.to_string().red());
                match self.verbose {
                    0 => eprintln!("{}", "try '--verbose' for more info".yellow().dimmed(),),
                    v if v < 2 => {
                        eprintln!("{}", "try '-vv' for even more info".yellow().dimmed(),)
                    }
                    v => {
                        let graph = runtime.graph();
                        let graph = graph.read().unwrap();
                        for line in spk::io::format_decisions(graph.walk().map(Ok), v) {
                            println!("{}", line?);
                        }
                    }
                }

                return Ok(1);
            }
            Err(err) => return Err(err.into()),
        };

        for item in solution.items() {
            if item.spec.pkg.name == request.pkg.name {
                serde_yaml::to_writer(std::io::stdout(), &*item.spec)
                    .context("Failed to serialize loaded spec")?;
                return Ok(0);
            }
        }
        tracing::error!("Internal Error: requested package was not in solution");
        Ok(1)
    }
}

impl View {
    fn print_current_env(&self) -> Result<i32> {
        let solution = spk::current_env()?;
        println!("{}", spk::io::format_solution(&solution, self.verbose));
        Ok(0)
    }

    fn print_variants_info(&self) -> Result<i32> {
        let (_, spec) = flags::find_package_spec(&self.package)
            .context("find package spec")?
            .must_be_found();

        for (index, variant) in spec.build.variants.iter().enumerate() {
            println!("{}: {}", index, variant);
        }

        Ok(0)
    }
}
