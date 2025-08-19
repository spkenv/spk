// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::any::Any;
use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use clap::Args;
use colored::Colorize;
use futures::{StreamExt, TryStreamExt};
use itertools::Itertools;
use miette::{bail, Context, IntoDiagnostic, Result};
use serde::Serialize;
use spfs::find_path::ObjectPathEntry;
use spfs::graph::{HasKind, ObjectKind};
use spfs::io::Pluralize;
use spfs::Digest;
use spk_cli_common::with_version_and_build_set::WithVersionSet;
use spk_cli_common::{
    current_env,
    flags,
    remove_ansi_escapes,
    CommandArgs,
    DefaultVersionStrategy,
    Run,
};
use spk_schema::foundation::format::{FormatChangeOptions, FormatRequest};
use spk_schema::foundation::option_map::OptionMap;
use spk_schema::foundation::spec_ops::Named;
use spk_schema::ident::Request;
use spk_schema::name::PkgNameBuf;
use spk_schema::version::Version;
use spk_schema::{
    AnyIdent,
    BuildIdent,
    Package,
    RequirementsList,
    Spec,
    Template,
    TestStage,
    Variant,
    VersionIdent,
};
use spk_solve::solution::{get_spfs_layers_to_packages, LayerPackageAndComponents};
use spk_solve::{PackageSource, Recipe, Solution, Solver, SolverMut};
use spk_storage;
use spk_storage::RepositoryHandle;
use strum::{Display, EnumString, IntoEnumIterator, VariantNames};

#[cfg(test)]
#[path = "./cmd_view_test.rs"]
mod cmd_view_test;

/// Constants for the valid output formats
#[derive(Default, Display, EnumString, VariantNames, Clone)]
#[strum(serialize_all = "lowercase")]
pub enum OutputFormat {
    Json,
    #[default]
    Yaml,
    EnvVars,
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
    #[clap(short = 'f', long)]
    pub format: Option<OutputFormat>,

    /// Explicitly get info on a filepath
    #[clap(short = 'F', long)]
    filepath: Option<String>,

    /// Explicitly get info on a package
    #[clap(short = 'p', long, conflicts_with_all = &["filepath"])]
    pkg: Option<String>,

    /// The package to show information about
    package: Option<String>,

    /// Display information about the variants defined by the package
    #[clap(long, group = "variants_info")]
    variants: bool,

    /// Display information about the variants defined by the package, including
    /// their defined tests.
    #[clap(long, group = "variants_info")]
    variants_with_tests: bool,

    // TODO: we can remove this, along with the solving call, once the
    // no solving method is bedded in.
    /// Use the older full solve method of finding the package info.
    /// The default is to not do a full solve.
    #[clap(long)]
    full_solve: bool,
}

#[async_trait::async_trait]
impl Run for View {
    type Output = i32;

    async fn run(&mut self) -> Result<Self::Output> {
        if self.variants || self.variants_with_tests {
            let options = self.options.get_options()?;
            let mut workspace = self
                .requests
                .workspace
                .load_or_default()
                .wrap_err("loading workspace")?;
            return self.print_variants_info(&options, &mut workspace, self.variants_with_tests);
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
            if let Ok(abspath) = dunce::canonicalize(package)
                && abspath.starts_with(spfs::env::SPFS_DIR)
            {
                return self.print_filepath_info(abspath.to_str().unwrap()).await;
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

#[derive(Serialize)]
struct PrintVariant<'a> {
    options: Cow<'a, OptionMap>,
    additional_requirements: Cow<'a, RequirementsList>,
}

#[derive(Serialize)]
struct PrintVariantWithTests<'a> {
    #[serde(flatten)]
    print_variant: PrintVariant<'a>,
    /// The number of tests per stage that would be run for this variant,
    /// considering any selectors defined.
    tests: BTreeMap<TestStage, u32>,
}

/// A helper for outputting solution data in non-pretty printed formats
#[derive(Serialize)]
struct ResolvedPackage {
    package: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    repo_name: Option<String>,
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    components: Option<Vec<String>>,
    version: String,
    highest: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    requesters: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<OptionMap>,
}

impl View {
    async fn solved_packages_output_data(
        &self,
        solution: &Solution,
        repos: &[Arc<RepositoryHandle>],
    ) -> Result<Vec<ResolvedPackage>> {
        let mut solved_packages = Vec::new();

        let resolved_items = if self.sort {
            solution.items().sorted_by_key(|item| item.spec.name())
        } else {
            // Convert back to vec once so that we can get an into_iter type
            solution.items().collect_vec().into_iter()
        };

        let highest_versions = solution.get_all_highest_package_versions(repos).await?;

        // Assemble output data structure, a subset of a full
        // solution. Should contain the same information as the
        // normal spk info output.
        for req in resolved_items {
            let package = remove_ansi_escapes(req.format_as_installed_package());
            let (repo_name, components) =
                if let PackageSource::Repository { repo, components } = &req.source {
                    (
                        Some(repo.name().to_string()),
                        Some(components.keys().map(ToString::to_string).collect()),
                    )
                } else {
                    (None, None)
                };
            let name = req.spec.ident().name().to_string();
            let version = req.spec.ident().version().to_string();
            let highest = match highest_versions.get(req.spec.name()) {
                Some(hv) => hv.to_string(),
                None => Version::default().to_string(),
            };

            let mut resolved_request = ResolvedPackage {
                package,
                repo_name,
                name,
                components,
                version,
                highest,
                size: None,
                requesters: None,
                options: None,
            };

            if self.verbose > 0 {
                let size = match &req.source {
                    PackageSource::Repository { repo, components } => {
                        match spk_storage::get_components_disk_usage(
                            repo.clone(),
                            Arc::new(req.spec.ident().clone()),
                            components,
                        )
                        .await
                        {
                            Ok(disk_usage) => disk_usage.size,
                            Err(err) => {
                                tracing::warn!(
                                    "Problem working out disk size of {}: {err}",
                                    req.spec.ident().to_string()
                                );
                                0
                            }
                        }
                    }
                    // Other package sources are ignored for disk usage
                    _ => 0,
                };
                let requesters = req
                    .request
                    .get_requesters()
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<String>>();
                let options = req.spec.option_values();

                resolved_request.size = Some(size);
                resolved_request.requesters = Some(requesters);
                resolved_request.options = Some(options);
            }

            solved_packages.push(resolved_request);
        }

        Ok(solved_packages)
    }

    async fn print_current_env(&self) -> Result<i32> {
        let solution = current_env().await?;
        let solver = self.solver.get_solver(&self.options).await?;

        if let Some(format) = &self.format {
            let solved_packages = self
                .solved_packages_output_data(&solution, solver.repositories())
                .await?;

            match format {
                OutputFormat::Yaml => serde_yaml::to_writer(std::io::stdout(), &solved_packages)
                    .into_diagnostic()
                    .wrap_err("Failed to serialize loaded spec")?,
                OutputFormat::Json => serde_json::to_writer(std::io::stdout(), &solved_packages)
                    .into_diagnostic()
                    .wrap_err("Failed to serialize loaded spec")?,
                OutputFormat::EnvVars => {
                    let env_vars = solution.to_environment::<HashMap<String, String>>(None);
                    for (name, value) in env_vars {
                        println!("{name}={value}");
                    }
                }
            }
        } else {
            // Solver solution output format
            println!(
                "{}",
                solution
                    .format_solution_with_highest_versions(
                        self.verbose,
                        solver.repositories(),
                        self.sort
                    )
                    .await?
            );
        }

        Ok(0)
    }

    fn print_variants_info(
        &self,
        options: &OptionMap,
        workspace: &mut spk_workspace::Workspace,
        show_variants_with_tests: bool,
    ) -> Result<i32> {
        let configured = match self.package.as_ref() {
            Some(name) => workspace.find_or_load_package_template(name),
            None => workspace.default_package_template().map_err(From::from),
        }
        .wrap_err("did not find recipe template")?;
        let rendered_data = configured.template.render(options)?;
        let recipe = rendered_data.into_recipe().wrap_err_with(|| {
            format!(
                "{filename} was expected to contain a recipe",
                filename = configured.template.file_path().to_string_lossy()
            )
        })?;

        let default_variants = recipe.default_variants(options);
        match &self.format {
            Some(format) => match format {
                OutputFormat::Yaml => tracing::warn!("No yaml format for variants"),
                OutputFormat::Json => {
                    let mut variants = BTreeMap::new();
                    let mut variants_with_tests = BTreeMap::new();

                    for (index, variant) in default_variants.iter().enumerate() {
                        let variant_info = PrintVariant {
                            options: variant.options(),
                            additional_requirements: variant.additional_requirements(),
                        };
                        if show_variants_with_tests {
                            let mut tests = BTreeMap::new();

                            for stage in TestStage::iter() {
                                let selected = recipe
                                    .get_tests(stage, variant)
                                    .wrap_err("Failed to select tests for this variant")?;
                                tests.insert(stage, selected.len() as u32);
                            }

                            variants_with_tests.insert(
                                index,
                                PrintVariantWithTests {
                                    print_variant: variant_info,
                                    tests,
                                },
                            );
                        } else {
                            variants.insert(index, variant_info);
                        }
                    }

                    if show_variants_with_tests {
                        serde_json::to_writer(std::io::stdout(), &variants_with_tests)
                            .into_diagnostic()
                            .wrap_err("Failed to serialize variant info")?
                    } else {
                        serde_json::to_writer(std::io::stdout(), &variants)
                            .into_diagnostic()
                            .wrap_err("Failed to serialize variant info")?
                    }
                }
                OutputFormat::EnvVars => tracing::warn!("No env vars format for variants"),
            },
            None if show_variants_with_tests => {
                tracing::warn!("--variants-with-tests requires json format");
            }
            None => {
                // Variants are not printed in yaml format
                for (index, variant) in default_variants.iter().enumerate() {
                    println!("{index}: {variant:#}");
                }
            }
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
            let layer_digest = match pathlist.iter().find(
                |item| matches!(item, ObjectPathEntry::Parent(o) if o.kind() == ObjectKind::Layer),
            ) {
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

    /// Display the contents of a package spec
    fn print_build_spec(&self, package_spec: Arc<Spec>) -> Result<i32> {
        match &self.format.clone().unwrap_or_default() {
            OutputFormat::Yaml => serde_yaml::to_writer(std::io::stdout(), &*package_spec)
                .into_diagnostic()
                .wrap_err("Failed to serialize loaded spec")?,
            OutputFormat::Json => serde_json::to_writer(std::io::stdout(), &*package_spec)
                .into_diagnostic()
                .wrap_err("Failed to serialize loaded spec")?,
            OutputFormat::EnvVars => tracing::warn!("No env vars format for variants"),
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

        let (parsed_request, _extra_options) = match self
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
                    return self.print_build_spec(package_spec);
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
            let ident: VersionIdent = temp_ident.to_version_ident();
            for repo in repos {
                match repo.read_recipe(&ident).await {
                    Ok(version_recipe) => {
                        match &self.format.clone().unwrap_or_default() {
                            OutputFormat::Yaml => {
                                serde_yaml::to_writer(std::io::stdout(), &*version_recipe)
                                    .into_diagnostic()
                                    .wrap_err("Failed to serialize loaded spec")?
                            }
                            OutputFormat::Json => {
                                serde_json::to_writer(std::io::stdout(), &*version_recipe)
                                    .into_diagnostic()
                                    .wrap_err("Failed to serialize loaded spec")?
                            }
                            OutputFormat::EnvVars => {
                                tracing::warn!("No env vars format for variants")
                            }
                        }
                        return Ok(0);
                    }
                    Err(err) => {
                        tracing::debug!("Unable to read recipe from {}: {err}", repo.name());

                        // Older repos can contain builds for a version, and have build specs,
                        // but not have a version recipe in the repo. In those cases, we show
                        // a build spec in lieu of a version recipe.
                        match repo.list_package_builds(&ident).await {
                            Ok(builds) if !builds.is_empty() => {
                                let build_ident = &builds[0];
                                match repo.read_package(build_ident).await {
                                    Ok(package_spec) => {
                                        let result = self.print_build_spec(package_spec);
                                        let number = builds.len();
                                        tracing::info!(
                                            "No version recipe exists. But found {number} {}. Output a build spec instead.",
                                            "build".pluralize(number)
                                        );
                                        return result;
                                    }
                                    Err(err) => {
                                        tracing::trace!("Unable to read package for build: {err}")
                                    }
                                }
                            }
                            Ok(_) => {}
                            Err(err) => tracing::trace!(
                                "{} repo has no builds for this version: {err}",
                                repo.name()
                            ),
                        }
                    }
                }
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
        repos: &[std::sync::Arc<spk_solve::RepositoryHandle>],
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

        // _extra_option are unused here because getting package info
        // from a solve is basically deprecated and should be removed soon.
        let (request, _extra_options) = match self
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

        let solution = if let Some(solver) =
            (&solver as &dyn Any).downcast_ref::<spk_solve::StepSolver>()
        {
            let mut runtime = solver.run();
            let formatter = self
                .solver
                .decision_formatter_settings
                .get_formatter(self.verbose)?;
            let result = formatter.run_and_print_decisions(&mut runtime).await;
            match result {
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
            }
        } else {
            solver.solve().await?
        };

        for item in solution.items() {
            if item.spec.name() == request.pkg.name {
                return self.print_build_spec(Arc::clone(&item.spec));
            }
        }

        tracing::error!("Internal Error: requested package was not in solution");
        Ok(1)
    }
}
