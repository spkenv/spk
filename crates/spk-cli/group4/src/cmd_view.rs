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
use spk_cli_common::with_version_and_build_set::WithVersionSet;
use spk_cli_common::{current_env, flags, CommandArgs, DefaultVersionStrategy, Run};
use spk_schema::foundation::format::{FormatChangeOptions, FormatRequest};
use spk_schema::foundation::option_map::OptionMap;
use spk_schema::foundation::spec_ops::Named;
use spk_schema::ident::Request;
use spk_schema::name::PkgNameBuf;
use spk_schema::version::Version;
use spk_schema::{AnyIdent, BuildIdent, Recipe, Template, VersionIdent};
use spk_solve::solution::{get_spfs_layers_to_packages, LayerPackageAndComponents};
use spk_storage;
use strum::{Display, EnumString, EnumVariantNames};

/// Constants for the valid output formats
#[derive(Display, EnumString, EnumVariantNames, Clone)]
#[strum(serialize_all = "lowercase")]
pub enum OutputFormat {
    Json,
    Yaml,
}

/// Show the spfs filepaths entry details at v > 0
const SHOW_SPFS_ENTRY_ONLY_LEVEL: u8 = 0;

/// Show the full spfs object path down to the filepath entry at v > 1
const SHOW_SPFS_FULL_TREE_LEVEL: u8 = 1;

/// Just show the first spfs object path down to the filepath entry
/// result when v < 1, and show them all at v >= 1
const SHOW_ALL_RESULTS_LEVEL: u8 = 1;

/// Don't show the additional settings fields when formatting a solved request
const DONT_SHOW_DETAILED_SETTINGS: u8 = 0;

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

    /// Sorts packages by package name in ascending order
    #[clap(short, long)]
    sort: bool,

    #[clap(short, long, global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// Format to output package data in
    #[clap(short = 'f', long, default_value_t = OutputFormat::Yaml)]
    pub format: OutputFormat,

    #[clap(flatten)]
    pub formatter_settings: flags::DecisionFormatterSettings,

    /// Explicitly get info on a filepath
    #[clap(short = 'F', long)]
    filepath: Option<String>,

    /// Explicitly get info on a package
    #[clap(short = 'p', long, conflicts_with_all = &["filepath"])]
    pkg: Option<String>,

    /// The package to show information about
    package: Option<String>,

    /// Display information about the variants defined by the package
    #[clap(long)]
    variants: bool,

    // TODO: we can remove this, along with the solving call, once the
    // no solving method is bedded in.
    /// Use the older full solve method of finding the package info.
    /// The default is to not do a full solve.
    #[clap(long)]
    full_solve: bool,
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

        if self.full_solve {
            // This is the older way. It runs a full solve. It's here
            // for backwards compatibility, and has to be opted-in to use.
            // TODO: we can remove this, along with the solving call, once the
            // no solving method is bedded in.
            self.print_package_info_from_solve(package).await
        } else {
            // Look up the requested package without doing a complete
            // dependency solve.  This is the newer way. It is the
            // default.
            self.print_package_info(package).await
        }
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
        let (_, template) = flags::find_package_template(self.package.as_ref())
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
        let mut in_a_runtime = true;
        let found = match spk_storage::find_path_providers(filepath).await {
            Ok(f) => f,
            Err(spk_storage::Error::SPFS(spfs::Error::NoActiveRuntime)) => {
                in_a_runtime = false;
                Vec::new()
            }
            Err(err) => return Err(err.into()),
        };

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
                            solved_request.repo_name().as_ref(),
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

    /// Display information on the package by looking up its
    /// specification or recipe directly based on these rules about
    /// what is in the given package identifier.
    ///
    /// ```txt
    /// spk info python <-- outputs the version spec for the latest python version
    /// spk info python/3 <!- error, no version spec for python/3 (show available versions)
    /// spk info python/3.7.3 <-- outputs the version spec
    /// spk info python/3.7.3/src <-- outputs the build spec
    /// spk info python/3.7.3/F4E632 <-- outputs the build spec
    /// ```
    async fn print_package_info(&self, package: &String) -> Result<i32> {
        let solver = self.solver.get_solver(&self.options).await?;
        let repos = solver.repositories();

        let parsed_request = match self
            .requests
            .parse_request(&package, &self.options, repos)
            .await
        {
            Ok(req) => req,
            Err(err) => {
                bail!("Was this meant to be a package or a filepath under /spfs? : {err}")
            }
        };

        let mut request = match parsed_request {
            Request::Pkg(pkg) => pkg,
            _ => bail!("Not a package request: {parsed_request:?}"),
        };

        // Request has a build, e.g.
        //   spk info python/3.7.3/src    --> output the build's spec
        //   spk info python/3.7.3/F4E632 --> output the build's spec
        if request.pkg.build.is_some() {
            let ident: BuildIdent = request.pkg.clone().try_into()?;
            for repo in repos {
                if let Ok(package_spec) = repo.read_package(&ident).await {
                    match &self.format {
                        OutputFormat::Yaml => {
                            serde_yaml::to_writer(std::io::stdout(), &*package_spec)
                                .context("Failed to serialize loaded spec")?
                        }
                        OutputFormat::Json => {
                            serde_json::to_writer(std::io::stdout(), &*package_spec)
                                .context("Failed to serialize loaded spec")?
                        }
                    }
                    return Ok(0);
                };
            }

            tracing::error!("Error: no such package/version/build found: {package}",);
            return Ok(1);
        }

        // Request is just a package name, e.g.
        //   spk info python --> output the version spec for the package's highest version
        request = request
            .with_version_or_else(DefaultVersionStrategy::Highest, repos)
            .await?;

        // Request is for a package/version, e.g.
        //   spk info python/3.7.3 --> output the version's spec (a recipe)
        if !request.pkg.version.is_empty() {
            let temp_ident: AnyIdent = request.pkg.clone().try_into()?;
            let ident: VersionIdent = temp_ident.to_version();
            for repo in repos {
                if let Ok(version_recipe) = repo.read_recipe(&ident).await {
                    match &self.format {
                        OutputFormat::Yaml => {
                            serde_yaml::to_writer(std::io::stdout(), &*version_recipe)
                                .context("Failed to serialize loaded spec")?
                        }
                        OutputFormat::Json => {
                            serde_json::to_writer(std::io::stdout(), &*version_recipe)
                                .context("Failed to serialize loaded spec")?
                        }
                    }
                    return Ok(0);
                };
            }
        }

        // Request is for a package/version that does not exist in the repos, e.g.
        //   spk info python/3   --> error, no version spec for python/3
        //   show list of versions instead
        tracing::info!("No version {} found for {package}", request.pkg.version);
        tracing::info!(
            "However, these versions are available for {}, some may be deprecated:",
            request.pkg.name
        );

        let name = request.pkg.name.clone();
        let versions = self.get_package_versions(&name, repos).await?;
        tracing::info!(
            "{}",
            versions
                .iter()
                .map(|v| format!("{name}/{v}"))
                .collect::<Vec<String>>()
                .join("\n")
        );

        Ok(0)
    }

    /// Helper to get all the versions for the given package name in these repo
    async fn get_package_versions(
        &self,
        name: &PkgNameBuf,
        repos: &Vec<std::sync::Arc<spk_solve::RepositoryHandle>>,
    ) -> Result<Vec<Version>> {
        let mut versions = Vec::new();
        for repo in repos {
            versions.extend(
                repo.list_package_versions(name)
                    .await?
                    .iter()
                    .map(|v| (**v).clone()),
            );
        }

        versions.sort();
        Ok(versions)
    }

    /// Original info gathering process using a solver to resolve the
    /// request for package and select the package build in the
    /// solution as the one to display information about.
    async fn print_package_info_from_solve(&self, package: &String) -> Result<i32> {
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
                match &self.format {
                    OutputFormat::Yaml => serde_yaml::to_writer(std::io::stdout(), &*item.spec)
                        .context("Failed to serialize loaded spec")?,
                    OutputFormat::Json => serde_json::to_writer(std::io::stdout(), &*item.spec)
                        .context("Failed to serialize loaded spec")?,
                }
                return Ok(0);
            }
        }

        tracing::error!("Internal Error: requested package was not in solution");
        Ok(1)
    }
}
