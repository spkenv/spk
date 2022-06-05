// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use anyhow::{bail, Context, Result};
use clap::Args;
use colored::Colorize;
use futures::{StreamExt, TryStreamExt};
use spk::prelude::*;

use super::{flags, CommandArgs, Run};

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

    #[clap(flatten)]
    pub formatter_settings: flags::DecisionFormatterSettings,

    /// The package to show information about
    package: Option<String>,

    /// Display information about the variants defined by the package
    #[clap(long)]
    variants: bool,
}

#[async_trait::async_trait]
impl Run for View {
    async fn run(&mut self) -> Result<i32> {
        if self.variants {
            let options = self.options.get_options()?;
            return self.print_variants_info(&options);
        }

        let package = match &self.package {
            None => return self.print_current_env().await,
            Some(p) => p,
        };

        let (mut solver, request) = tokio::try_join!(
            self.solver.get_solver(&self.options),
            self.requests.parse_request(&package, &self.options)
        )?;
        solver.add_request(request.clone());
        let request = match request {
            spk::api::Request::Pkg(pkg) => pkg,
            _ => bail!("Not a package request: {request:?}"),
        };

        let mut runtime = solver.run();

        let formatter = self.formatter_settings.get_formatter(self.verbose);

        let solution = formatter.run_and_print_decisions(&mut runtime).await;
        let solution = match solution {
            Ok(s) => s,
            Err(err @ spk::Error::Solve(_)) => {
                println!("{}", err.to_string().red());
                match self.verbose {
                    0 => eprintln!("{}", "try '--verbose' for more info".yellow().dimmed(),),
                    v if v < 2 => {
                        eprintln!("{}", "try '-vv' for even more info".yellow().dimmed(),)
                    }
                    _v => {
                        let graph = runtime.graph();
                        let graph = graph.read().await;
                        // Iter much?
                        let mut graph_walk = graph.walk();
                        let walk_iter = graph_walk.iter().map(Ok);
                        let mut decision_iter = formatter.formatted_decisions_iter(walk_iter);
                        let iter = decision_iter.iter();
                        tokio::pin!(iter);
                        while let Some(line) = iter.try_next().await? {
                            println!("{line}");
                        }
                    }
                }

                return Ok(1);
            }
            Err(err) => return Err(err.into()),
        };

        for item in solution.items() {
            if item.spec.name() == &request.pkg.name {
                serde_yaml::to_writer(std::io::stdout(), &*item.spec)
                    .context("Failed to serialize loaded spec")?;
                return Ok(0);
            }
        }
        tracing::error!("Internal Error: requested package was not in solution");
        Ok(1)
    }
}

impl CommandArgs for View {
    fn get_positional_args(&self) -> Vec<String> {
        // The important positional arg for a view/info is the package
        match &self.package {
            Some(pkg) => vec![pkg.clone()],
            None => vec![],
        }
    }
}

impl View {
    async fn print_current_env(&self) -> Result<i32> {
        let solution = spk::current_env().await?;
        println!("{}", spk::io::format_solution(&solution, self.verbose));
        Ok(0)
    }

    fn print_variants_info(&self, options: &spk::api::OptionMap) -> Result<i32> {
        let (_, template) = flags::find_package_template(&self.package)
            .context("find package template")?
            .must_be_found();
        let recipe = template.render(options)?;

        for (index, variant) in recipe.default_variants().iter().enumerate() {
            println!("{}: {}", index, variant);
        }

        Ok(0)
    }
}
