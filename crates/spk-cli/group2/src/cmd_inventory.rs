// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::time::Instant;

use clap::Args;
use itertools::Itertools;
use miette::{IntoDiagnostic, Result, WrapErr};
use serde::Serialize;
use spk_cli_common::{CommandArgs, Run, flags};
use spk_graph::{PackageConnections, RepoPackageConnections};
use spk_solve::option_map::get_host_options_filters;
use spk_storage::RepoPackageDependencies;
use strum::{Display, EnumString, VariantNames};

use crate::cmd_ls::{Console, Output};

/// Sentinel value for the starting point used to detect depth changes
/// when outputting packages grouped by depth.
const NO_DEPTH_SEEN: u32 = u32::MAX;

// Output formatting constants
const DEPS_EXTRA_PADDING: usize = 32;
const USED_BY_EXTRA_PADDING: usize = 23;

#[cfg(test)]
#[path = "./cmd_inventory_test.rs"]
mod cmd_inventory_test;

/// Constants for non-pretty printed output formats
#[derive(Default, Display, EnumString, VariantNames, Clone)]
#[strum(serialize_all = "lowercase")]
pub enum OutputFormat {
    Json,
    #[default]
    Yaml,
}

// Helper structs for yaml, json output formats

/// Package info for a package being output by depth
#[derive(Default, Serialize)]
pub struct OutputPackage {
    pub name: String,
    pub depth: u32,
    /// Direct dependencies are from a package's spec file. Deeper or
    /// transitive dependencies are not included in this list.
    pub direct_deps: Vec<String>,
    /// Number of other packages that use this one (have it as a
    /// dependency).
    pub num_clients: usize,
    /// Total number of direct and deeper/transitive dependencies this
    /// package has.
    pub num_all_deps: usize,
}

/// For outputting lists of packages at each depth.
#[derive(Default, Serialize)]
pub struct OutputListsByDepth {
    pub depths: Vec<Vec<OutputPackage>>,
}

/// Package info for a package being output in a list of dependencies,
/// or clients/used by packages.
#[derive(Default, Serialize)]
pub struct OutputDependencyPackage {
    pub name: String,
    pub depth: u32,
    pub direct: bool,
}

/// For outputting a list of packages as dependencies of another package.
#[derive(Default, Serialize)]
pub struct OutputDepsList {
    pub dependencies: Vec<OutputDependencyPackage>,
}

/// For outputting a list of packages that use another package (its
/// clients or what it is used by).
#[derive(Default, Serialize)]
pub struct OutputUsedByList {
    pub used_by: Vec<OutputDependencyPackage>,
}

/// Dependency analysis of packages in one or more repositories.
#[derive(Args)]
#[clap(visible_alias = "analysis")]
pub struct Inventory<Output: Default = Console> {
    #[clap(flatten)]
    pub repos: flags::Repositories,

    #[clap(short, long, global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// Disable the filtering that would only show items that have a
    /// build that matches the current host's host options. This
    /// option can be configured as the default in spk's config file.
    #[clap(long, conflicts_with = "host")]
    pub(crate) no_host: bool,

    /// Enable filtering to only show items that have a build that
    /// matches the current host's host options. This option can be
    /// configured as the default in spk's config file.
    #[clap(long)]
    pub(crate) host: bool,

    /// Format to output package data in. If not specified a human
    /// readable format will be used.
    #[clap(short = 'f', long)]
    pub format: Option<OutputFormat>,

    /// Name of the package to gather dependency information on.
    /// When not given, the command gathers information on all
    /// packages in the repository.
    #[clap(name = "NAME")]
    package: Option<String>,

    /// Show a list of the packages that use the given package instead
    /// of the used by packages and dependencies at each depth.
    /// Requires a package to be specified.
    #[clap(long)]
    used_by: bool,

    /// Show a list of the dependencies of the given package instead
    /// of the used by packages and dependencies at each depth.
    /// Requires a package to be specified.
    #[clap(long)]
    deps: bool,

    #[clap(skip)]
    pub(crate) output: Output,
}

#[async_trait::async_trait]
impl<T: Output> Run for Inventory<T> {
    type Output = i32;

    async fn run(&mut self) -> Result<Self::Output> {
        let config = spk_config::get_config()?;
        if config.cli.ls.host_filtering {
            if !self.no_host {
                self.host = true;
            }
        } else if !self.host {
            self.no_host = true;
        }

        let repos = self.repos.get_repos_for_non_destructive_operation().await?;

        // TODO: this is repeated in spk search, move to flags.rs?
        // along with the config+host checks done above?  Set the
        // default filter to the all current host's host options
        // (--host). --no-host will disable this.
        let filter_by = if !self.no_host && self.host {
            get_host_options_filters()
        } else {
            None
        };
        tracing::trace!("Filter is: {:?}", filter_by);

        // Get the data for all the packages from the repos
        let start = Instant::now();

        let repo_deps_data = RepoPackageDependencies::from_repos(&repos, &filter_by).await?;
        let all_packages_connections =
            RepoPackageConnections::from_repo_dependencies(&repo_deps_data);

        println!(
            "Gathered package data from repos in   : {} secs",
            start.elapsed().as_secs_f64()
        );

        let package_option = self.package.clone();
        if let Some(package) = package_option {
            // Focus on a single package. Show 3 sets of data:
            // - the package's dependencies grouped by depth
            // - a list of all the package's dependencies, ordered by name
            // - a list of all the other packages that use this
            //   package (its clients), ordered by name
            match all_packages_connections.get(&package) {
                Some(single_package) => {
                    if self.format.is_none() {
                        self.output.println(format!("Focusing on: {package}"));
                    }

                    let package_deps = all_packages_connections.get_all_deps(single_package);

                    if self.used_by || self.deps {
                        if self.used_by {
                            // Show a list of the users/parents of
                            // this package (their package names)
                            let used_by_packages =
                                all_packages_connections.get_all_used_by(single_package);
                            self.output
                                .println(format!("\nPackages that use '{package}':"));

                            let output_format = self.format.clone();
                            if let Some(format) = output_format {
                                let used_by_names: Vec<_> = used_by_packages
                                    .iter()
                                    .map(|ub| ub.name.to_string())
                                    .collect();
                                self.output_formatted_data(
                                    &format,
                                    &used_by_names,
                                    "Failed to serialize clients/used by data",
                                )?;
                            } else {
                                for used_by in used_by_packages {
                                    self.output.println(used_by.name.to_string());
                                }
                            }
                        }
                        if self.deps {
                            // Show a list of the dependencies (their package names)
                            self.output
                                .println(format!("\nPackages that '{package}' depends on:"));
                            let output_format = self.format.clone();
                            if let Some(format) = output_format {
                                let dep_names: Vec<_> =
                                    package_deps.iter().map(|d| d.name.to_string()).collect();
                                self.output_formatted_data(
                                    &format,
                                    &dep_names,
                                    "Failed to serialize package dependencies data",
                                )?;
                            } else {
                                for dep in package_deps {
                                    self.output.println(dep.name.to_string());
                                }
                            }
                        }
                    } else {
                        self.show_packages_at_each_depth(&package_deps, &all_packages_connections)?;
                        self.show_all_deps_of_package(single_package, &all_packages_connections)?;
                        self.show_all_packages_that_use_package(
                            single_package,
                            &all_packages_connections,
                        )?;
                    }
                }
                _ => {
                    self.output.println(format!("No '{package}' found."));
                    return Ok(1);
                }
            }
        } else {
            // No package given, so show all packages grouped by their depths
            self.show_packages_at_each_depth(
                &all_packages_connections.get_all_packages(),
                &all_packages_connections,
            )?;
        }

        Ok(0)
    }
}

impl<T: Output> Inventory<T> {
    /// Output data according to the given output format
    fn output_formatted_data<S>(
        &mut self,
        format: &OutputFormat,
        data: &S,
        error_context: &'static str,
    ) -> Result<()>
    where
        S: ?Sized + serde::ser::Serialize,
    {
        match format {
            OutputFormat::Yaml => {
                let yaml_data = serde_yaml::to_string(data)
                    .into_diagnostic()
                    .wrap_err(error_context)?;
                self.output.println(yaml_data);
            }
            OutputFormat::Json => {
                let json_data = serde_json::to_string(data)
                    .into_diagnostic()
                    .wrap_err(error_context)?;
                self.output.println(json_data);
            }
        }
        Ok(())
    }

    /// Prints a list of all the package's dependencies, grouped by
    /// depth in the repo.
    fn show_packages_at_each_depth(
        &mut self,
        packages_to_show: &[PackageConnections],
        all_packages: &RepoPackageConnections,
    ) -> Result<()> {
        // Example of output:
        //
        // DEPTH 0
        // ------------------------------
        // cuda (clients=15, xdeps=0) deps = []
        // gcc (clients=527, xdeps=0) deps = []
        // ...
        //
        // DEPTH 1
        // ------------------------------
        // cmake (clients=483, xdeps=0) deps = ['gcc']
        // ...
        //
        let mut last_depth = NO_DEPTH_SEEN;

        // Want the packages ordered by depth and name
        let sorted_pkgs: Vec<&PackageConnections> = packages_to_show
            .iter()
            .sorted_by_key(|data| (data.depth, data.name.clone()))
            .collect();

        let output_format = self.format.clone();
        if let Some(format) = output_format {
            // Assemble a data structure of lists that contain the
            // packages at each depth
            let mut depth_data = OutputListsByDepth::default();
            let mut depth_list = Vec::new();

            for package in sorted_pkgs {
                if last_depth != package.depth {
                    if !depth_list.is_empty() {
                        depth_data.depths.push(depth_list);
                        depth_list = Vec::new();
                    }
                    last_depth = package.depth;
                }

                let output_pkg = OutputPackage {
                    name: package.name.clone(),
                    depth: package.depth,

                    direct_deps: all_packages.get_direct_deps_names(package),
                    num_clients: package.direct_used_by.len(),
                    num_all_deps: package.all_deps.len(),
                };
                depth_list.push(output_pkg);
            }

            self.output_formatted_data(
                &format,
                &depth_data,
                "Failed to serialize package connections data grouped by depth",
            )?;
        } else {
            // Pretty-print formatted output for humans
            for package in sorted_pkgs {
                // Heading lines are output at every depth change
                if last_depth != package.depth {
                    self.output.println(format!(
                        "\n\nDEPTH {}\n------------------------------",
                        package.depth - 1
                    ));
                    last_depth = package.depth;
                }

                // Non-heading lines for a package at the current depth
                self.output.println(format!(
                    "{} (clients={}, xdeps={}) deps = [{}]",
                    package.name,
                    package.direct_used_by.len(),
                    // The script this is based on did not count the
                    // direct dependencies in the number of xdeps
                    // (extra/transitive deps).
                    package.all_deps.len() - package.direct_deps.len(),
                    all_packages
                        .get_direct_deps_names(package)
                        .iter()
                        .join(", ")
                ));
            }
        }
        Ok(())
    }

    /// Prints a list of all the package's dependencies, ordered by
    /// name, showing their depths and whether or not they are direct
    /// dependencies.
    fn show_all_deps_of_package(
        &mut self,
        package: &PackageConnections,
        all_packages: &RepoPackageConnections,
    ) -> Result<()> {
        // Example of output:
        //
        // All transitive dependencies of zlib:
        // ------------------------------------
        //  - gcc     (depth 0)
        // ...

        // Dependencies ordered by name
        let sorted_pkgs = all_packages.get_all_deps(package);

        let output_format = self.format.clone();
        if let Some(format) = output_format {
            // Assemble a list of all dependencies to make formatted
            // output easier.
            let mut output_deps = OutputDepsList::default();
            for pkg_data in sorted_pkgs.iter() {
                let pkg = OutputDependencyPackage {
                    name: pkg_data.name.clone(),
                    depth: pkg_data.depth,
                    direct: package.direct_deps_contains(pkg_data),
                };
                output_deps.dependencies.push(pkg);
            }

            self.output_formatted_data(
                &format,
                &output_deps,
                "Failed to serialize package dependencies data",
            )?;
        } else {
            // Pretty-print formatted output for humans
            self.output.println(format!(
                "\nAll transitive dependencies of {}:",
                package.name
            ));
            self.output.println(
                "-".repeat(package.name.len() + DEPS_EXTRA_PADDING)
                    .to_string(),
            );

            let mut max_name_len = 0;
            for pkg_data in sorted_pkgs.iter() {
                max_name_len = std::cmp::max(max_name_len, pkg_data.name.len());
            }

            for pkg_data in sorted_pkgs.iter() {
                self.output.println(format!(
                    "  - {:max_name_len$}  (depth: {})",
                    pkg_data.name,
                    pkg_data.depth - 1
                ));
            }
        }

        Ok(())
    }

    /// Prints a list of all the package's user, the other packages
    /// that rely on this one, ordered by name, showing their depths
    /// and whether or not they directly use the package.
    fn show_all_packages_that_use_package(
        &mut self,
        package: &PackageConnections,
        all_packages: &RepoPackageConnections,
    ) -> Result<()> {
        // Example of output:
        //
        // All packages that use zlib:
        // ---------------------------
        // - absorb                         (dir) (depth 12)
        // - alembic                              (depth 6)
        // ...

        // Package that use this package, ordered by name
        let sorted_pkgs = all_packages.get_all_used_by(package);

        let output_format = self.format.clone();
        if let Some(format) = output_format {
            // Assemble a list of all the client packages (packages
            // that use the given package) to make formatted output
            // easier.
            let mut output_used_by = OutputUsedByList::default();
            for pkg_data in sorted_pkgs.iter() {
                let pkg = OutputDependencyPackage {
                    name: pkg_data.name.clone(),
                    depth: pkg_data.depth,
                    direct: package.direct_used_by.contains(&pkg_data.node_index),
                };
                output_used_by.used_by.push(pkg);
            }

            self.output_formatted_data(
                &format,
                &output_used_by,
                "Failed to serialize package clients/used by data",
            )?;
        } else {
            // Pretty-print formatted output for humans
            self.output
                .println(format!("\nAll packages that use {}:", package.name));
            self.output.println(
                "-".repeat(package.name.len() + USED_BY_EXTRA_PADDING)
                    .to_string(),
            );

            let mut max_name_len = 0;
            for pkg_data in sorted_pkgs.iter() {
                max_name_len = std::cmp::max(max_name_len, pkg_data.name.len());
            }

            for pkg_data in sorted_pkgs.iter() {
                // Direct dependencies are marked, deeper, or transitive,
                // dependencies are not.
                let direct_user_of = if package.direct_used_by.contains(&pkg_data.node_index) {
                    "(dir)"
                } else {
                    "     "
                };
                self.output.println(format!(
                    "  - {:max_name_len$}  {direct_user_of} (depth: {})",
                    pkg_data.name,
                    pkg_data.depth - 1,
                ));
            }
        }
        Ok(())
    }
}

impl<T: Output> CommandArgs for Inventory<T> {
    fn get_positional_args(&self) -> Vec<String> {
        // The important positional args for inventory are the packages
        match &self.package {
            Some(pkg) => vec![pkg.clone()],
            None => vec![],
        }
    }
}
