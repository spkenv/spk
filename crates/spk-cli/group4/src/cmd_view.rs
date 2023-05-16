// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use clap::Args;
use colored::Colorize;
use futures::{StreamExt, TryStreamExt};
use spfs::find_path::ObjectPathEntry;
use spfs::graph::Object;
use spfs::Digest;
use spk_cli_common::{current_env, flags, CommandArgs, Run};
use spk_schema::foundation::format::{FormatChangeOptions, FormatRequest};
use spk_schema::foundation::option_map::OptionMap;
use spk_schema::foundation::spec_ops::Named;
use spk_schema::ident::Request;
use spk_schema::{Recipe, Template};
use spk_solve::solution::{get_spfs_layers_to_packages, LayerPackageAndComponents};

/// Show the spfs filepaths entry details at v > 0
const SHOW_SPFS_ENTRY_ONLY_LEVEL: u32 = 0;

/// Show the full spfs object path down to the filepath entry at v > 1
const SHOW_SPFS_FULL_TREE_LEVEL: u32 = 1;

/// Just show the first spfs object path down to the filepath entry
/// result when v < 1, and show them all at v >= 1
const SHOW_ALL_RESULTS_LEVEL: u32 = 1;

/// Don't show the additional settings fields when formatting a solved request
const DONT_SHOW_DETAILED_SETTINGS: u32 = 0;

/// Don't format a solved request as an initial request
const NOT_AN_INITIAL_REQUEST: u64 = 1;

/// View the current environment, or information about a package, or filepath under /spfs
#[derive(Args)]
#[clap(visible_aliases = &["info", "provides"])]
pub struct View {
    #[clap(flatten)]
    requests: flags::Requests,
    #[clap(flatten)]
    options: flags::Options,
    #[clap(flatten)]
    solver: flags::Solver,

    #[clap(short, long, global = true, parse(from_occurrences))]
    pub verbose: u32,

    #[clap(flatten)]
    pub formatter_settings: flags::DecisionFormatterSettings,

    /// Explicity get info on a filepath
    #[clap(short = 'F', long)]
    filepath: Option<String>,

    /// Explicity get info on a package
    #[clap(short = 'p', long, conflicts_with_all = &["filepath"])]
    pkg: Option<String>,

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

        let package = match (&self.package, &self.filepath, &self.pkg) {
            (None, Some(fp), _) => {
                // No bareword package request given, but was there a
                // -F <filepath> option, e.g. 'spk info -F some/path/'
                fp
            }
            (None, None, Some(p)) => {
                // No bareword package request given, but was there a
                // -p <pkg> option, e.g. 'spk info -p some_package
                p
            }
            (None, None, None) => {
                // No package, filepath or pkg options given
                // e.g. 'spk info'
                return self.print_current_env().await;
            }
            // A bareword request, e.g. 'spk info package_or_filepath'
            (Some(p), _, _) => p,
        };

        // For 'spk info /spfs/file/path' or 'spk info -F
        // /spfs/file/path' invocations, given a filepath work out
        // which package(s) and spfs layers provide it.
        if self.pkg.is_none() {
            if let Ok(abspath) = std::fs::canonicalize(PathBuf::from(package)) {
                if abspath.starts_with(spfs::env::SPFS_DIR) {
                    return self.print_filepath_info(abspath.to_str().unwrap()).await;
                }
            }
            if self.filepath.is_some() {
                // This was given as a filepath but there
                // isn't a matching file on disk under /spfs
                bail!("Path does not exist under /spfs: {package}");
            }
            // Otherwise fall through to trying to get info on the
            // value as a package.
        }

        let mut solver = self.solver.get_solver(&self.options).await?;

        let request = match self
            .requests
            .parse_request(&package, &self.options, solver.repositories())
            .await
        {
            Ok(req) => req,
            Err(err) => {
                bail!("Was this meant to be a package or a filepath under /spfs? : {err}")
            }
        };

        solver.add_request(request.clone());
        let request = match request {
            Request::Pkg(pkg) => pkg,
            _ => bail!("Not a package request: {request:?}"),
        };

        let mut runtime = solver.run();

        let formatter = self.formatter_settings.get_formatter(self.verbose)?;

        let result = formatter.run_and_print_decisions(&mut runtime).await;
        let solution = match result {
            Ok((s, _)) => s,
            Err(err) => {
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
        };

        for item in solution.items() {
            if item.spec.name() == request.pkg.name {
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
        let solution = current_env().await?;
        let solver = self.solver.get_solver(&self.options).await?;
        println!(
            "{}",
            solution
                .format_solution_with_highest_versions(self.verbose, solver.repositories())
                .await?
        );
        Ok(0)
    }

    fn print_variants_info(&self, options: &OptionMap) -> Result<i32> {
        let (_, template) = flags::find_package_template(&self.package)
            .context("find package template")?
            .must_be_found();
        let recipe = template.render(options)?;

        let default_variants = recipe.default_variants();
        for (index, variant) in default_variants.iter().enumerate() {
            println!("{index}: {variant:#}");
        }

        Ok(0)
    }

    /// Given a filepath inside /spfs, print out the package(s) and spfs entries for it.
    async fn print_filepath_info(&self, filepath: &str) -> Result<i32> {
        // First, we need a list of all the providing pathlists that
        // contain this file. Each of them should contain at least a
        // layer and the filepath entry, but might contain other spfs
        // objects.
        let (in_a_runtime, found) = spk_storage::find_path_providers(filepath).await?;

        if found.is_empty() {
            println!("{filepath}: {}", "not found".yellow());
            println!(
                " - {}",
                if in_a_runtime {
                    "not found in current /spfs runtime".yellow()
                } else {
                    "No active runtime".red()
                }
            );
            return Ok(1);
        }

        // The layers need to be pulled out of each pathlist in order
        // to match them against the packages from the runtime later.
        let mut layers_that_contain_filepath = BTreeMap::new();
        let mut stack_order: Vec<Digest> = Vec::new();
        for pathlist in found.iter() {
            let layer_digest = match pathlist
                .iter()
                .find(|item| matches!(item, ObjectPathEntry::Parent(Object::Layer(_))))
            {
                Some(l) => l.digest()?,
                None => {
                    return Err(spk_cli_common::Error::String(
                        "Path list entry does not contain a layer. This cannot happen, all the entries should contain a layer.".to_string(),
                     ).into());
                }
            };
            layers_that_contain_filepath.insert(layer_digest, pathlist);
            stack_order.push(layer_digest);
        }

        // We need a mapping of layers to packages from the current
        // runtime to find the packages that provide the layers that
        // provide the filepath.
        let solution = current_env().await?;
        let items = solution.items();
        let layers_to_packages = get_spfs_layers_to_packages(&items)?;

        // Now we can display the package and spfs data about the
        // filepath. It is possible for a filepath to be provided by
        // multiple layers and packages, and it is also possible for
        // the layer(s) have no package related to them (e.g. layers
        // created via spfs commands directly).
        let number = layers_that_contain_filepath.len();
        println!(
            "{}: is in {} {}{}:",
            filepath.green(),
            number,
            if number > 1 {
                "packages/layers"
            } else {
                "package/layer"
            },
            if self.verbose < SHOW_ALL_RESULTS_LEVEL && number > 1 {
                ", topmost 1 shown, use -v to see all"
            } else {
                ""
            }
        );

        // Stack order is used to ensure the packages and layers are
        // shown top down, from packages added later first to packages
        // added earlier below them.
        for layer_digest in stack_order.iter() {
            let pathlist = match layers_that_contain_filepath.get(layer_digest) {
                Some(pl) => pl,
                None => {
                    let message = "Missing pathlist for layer digest known to contain the filepath. This cannot happen, layers_that_contain_filepath should contain an entry for each layer digest in stack_order".to_string();
                    tracing::error!(message);
                    return Err(spk_cli_common::Error::String(message).into());
                }
            };

            match layers_to_packages.get(layer_digest) {
                Some(LayerPackageAndComponents(solved_request, _component)) => {
                    println!(
                        " {}",
                        solved_request.request.format_request(
                            &solved_request.repo_name(),
                            &solved_request.request.pkg.name,
                            &FormatChangeOptions {
                                verbosity: DONT_SHOW_DETAILED_SETTINGS,
                                level: NOT_AN_INITIAL_REQUEST,
                            }
                        )
                    )
                }
                None => {
                    // There is no matching spk package for this
                    // layer, but it does provide the file
                    println!(
                        "Unknown spk package{}",
                        if self.verbose < SHOW_SPFS_FULL_TREE_LEVEL {
                            ". Re-run with more '-v's to see the spfs data"
                        } else {
                            ", but the spfs data is:"
                        }
                    );
                }
            };

            // The spfs details are only shown at higher verbosity levels
            if self.verbose > SHOW_SPFS_ENTRY_ONLY_LEVEL {
                if self.verbose > SHOW_SPFS_FULL_TREE_LEVEL {
                    spk_storage::pretty_print_filepath(filepath, pathlist).await?;
                } else {
                    // This will have a last entry because it
                    // represents a path through the spfs object trees
                    // to the filepath, and at least one such path
                    // much have been found above for the code to be
                    // reached.
                    let entry_only = match pathlist.last() {
                        Some(entry) => vec![entry.clone()],
                        None => {
                            let message = "Pathlist does not contain a last entry. This cannot happen, all the pathlists found should contain at least an entry for the filepath.".to_string();
                            tracing::error!(message);
                            return Err(spk_cli_common::Error::String(message).into());
                        }
                    };
                    spk_storage::pretty_print_filepath(filepath, &entry_only).await?;
                };
            }

            // Only show all the found entries at higher verbosity levels.
            if self.verbose < SHOW_ALL_RESULTS_LEVEL {
                break;
            }
        }

        Ok(0)
    }
}
