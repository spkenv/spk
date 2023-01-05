// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::HashMap;
use std::sync::Arc;

use rug::Integer;
use spk_schema::ident::PkgRequest;
use spk_schema::name::PkgNameBuf;
use spk_schema::{BuildIdent, Deprecate, Package, VersionIdent};
use spk_solve_solution::Solution;
use spk_storage::RepositoryHandle;

use crate::{Error, Result};

// Totals with more that this number of digits will be shown using
// scientific notation
const DIGITS_LIMIT: usize = 10;

// Verbosity level that show more details
const SHOW_FULL_DIGITS_LEVEL: u32 = 1;
const SHOW_FULL_CALCULATION_LEVEL: u32 = 2;

pub async fn show_search_space_stats(
    initial_requests: &[String],
    solution: &Solution,
    repos: &[Arc<RepositoryHandle>],
    verbosity: u32,
) -> Result<()> {
    println!("Calculating search space stats. This may take some time...");

    // Get names of all packages from solve, in the order they where
    // resolved. Keeping this order is important for the search space
    // size calculation.
    let solution_packages = solution
        .packages_in_solve_order()
        .iter()
        .map(|spec| spec.ident().clone())
        .collect::<Vec<BuildIdent>>();
    let names = solution_packages.iter().map(|p| p.name().into()).collect();

    // Get the all the builds, of all the versions, of all the named
    // packages so they can be used to compute the search space
    let data = get_package_version_build_states(&names, repos).await?;

    // Get the requests that made the solution so they be used to
    // refine some of the search space numbers
    let solution_requests = solution
        .items()
        .map(|sr| (sr.request.pkg.name.clone(), sr.request.clone()))
        .collect::<HashMap<PkgNameBuf, PkgRequest>>();

    display_search_space_stats(
        initial_requests,
        &solution_packages,
        &data,
        verbosity,
        &solution_requests,
    );
    Ok(())
}

async fn get_package_version_build_states(
    package_names: &Vec<PkgNameBuf>,
    repos: &[Arc<RepositoryHandle>],
) -> Result<Vec<(BuildIdent, bool, bool)>> {
    // A list of (package ident, is it deprecated?, is it source?)
    let mut data: Vec<(BuildIdent, bool, bool)> = Vec::new();

    for repo in repos.iter() {
        for package in package_names {
            let versions = match repo.list_package_versions(package).await {
                Ok(v) => v,
                Err(err) => return Err(Error::String(err.to_string())),
            };

            for version in versions.iter() {
                let pkg_version = VersionIdent::new((*package).clone(), (**version).clone());

                let builds = match repo.list_package_builds(&pkg_version).await {
                    Ok(b) => b,
                    Err(err) => return Err(Error::String(err.to_string())),
                };

                for build in builds {
                    let is_src = build.is_source();

                    let spec = match repo.read_package(&build).await {
                        Ok(s) => s,
                        Err(err) => return Err(Error::String(err.to_string())),
                    };
                    let is_deprecated = spec.is_deprecated();

                    data.push((build.clone(), is_deprecated, is_src));
                }
            }
        }
    }

    Ok(data)
}

fn display_search_space_stats(
    initial_requests: &[String],
    packages: &Vec<BuildIdent>,
    extracted_data: &Vec<(BuildIdent, bool, bool)>,
    verbosity: u32,
    requests: &HashMap<PkgNameBuf, PkgRequest>,
) {
    // Count what was extracted by package name based on the builds
    // and their statuses.
    let mut total_counters: HashMap<PkgNameBuf, u64> = HashMap::new();
    let mut counters: HashMap<PkgNameBuf, u64> = HashMap::new();
    let mut without_embedded_counters: HashMap<PkgNameBuf, u64> = HashMap::new();
    let mut version_range_counters: HashMap<PkgNameBuf, u64> = HashMap::new();

    for (ident, is_deprecated, is_src) in extracted_data {
        let pkg_name = ident.name();

        // All builds
        let counter = total_counters.entry(pkg_name.into()).or_insert(0);
        *counter += 1;

        // Active, non-src builds
        if !*is_deprecated && !*is_src {
            let counter = counters.entry(pkg_name.into()).or_insert(0);
            *counter += 1;
        }

        // Active, non-src, non-embedded builds
        if !*is_deprecated && !*is_src & !ident.is_embedded() {
            let counter = without_embedded_counters
                .entry(pkg_name.into())
                .or_insert(0);
            *counter += 1;
        }

        // Active, version range matching (so only those that fall within
        // the request's version range), non-src, non-embedded builds
        if !*is_deprecated && !*is_src & !ident.is_embedded() {
            if let Some(request) = requests.get(pkg_name) {
                if request.is_version_applicable(ident.version()).is_ok() {
                    let counter = version_range_counters.entry(pkg_name.into()).or_insert(0);
                    *counter += 1;
                }
            }
        }
    }

    tracing::info!(
        "===== Search space stats for: '{}' =====",
        initial_requests.join(", ")
    );
    let depth = packages.len();
    tracing::info!(" {depth} levels in the decision tree");
    tracing::info!(" {depth} packages in the solution");

    tracing::info!("----- All builds, including deprecated and src --------------------");
    show_total_stats(&total_counters, packages, verbosity);
    tracing::info!("----- Only Active, non-src builds ---------------------------------");
    show_total_stats(&counters, packages, verbosity);
    tracing::info!("----- Active, non-src, non-embedded builds ------------------------");
    show_total_stats(&without_embedded_counters, packages, verbosity);
    tracing::info!("----- Active, in version range, non-src, non-embedded builds ------");
    show_total_stats(&version_range_counters, packages, verbosity);
}

fn show_total_stats(
    counters: &HashMap<PkgNameBuf, u64>,
    packages: &Vec<BuildIdent>,
    verbosity: u32,
) {
    // Show some calculation estimates about the decision tree's nodes
    // based on the package data. These are "extreme unlikely to
    // happen" upper limits because they do not account for all the
    // dependency subsets, reordering, compat ranges, or per-version
    // expansion of the solver. But they represent the number of nodes
    // that would have to be expanded and verified if the entire
    // search tree was visited by a brute force search.
    let mut calc: String = "0".to_string();
    let mut total = Integer::new();
    let mut total_builds = Integer::new();
    let mut avg_branches = Integer::new();
    let mut previous_num_nodes: Integer = Integer::new() + 1;

    for pkg in packages {
        let name = pkg.name();
        if let Some(number) = counters.get(name) {
            total_builds += number;
            avg_branches += number;

            let new_nodes = previous_num_nodes.clone() * number;
            total += new_nodes.clone();
            // Using 1 + 1*n + (1*n)*m + ((1*n)*m)*p + ... for the
            // details to make the calculations clearer.
            calc += &format!(" +\n {previous_num_nodes}*{number} {name} (so far: {total})");
            previous_num_nodes = new_nodes;
        }
    }

    avg_branches /= packages.len() as u128;

    if verbosity > SHOW_FULL_CALCULATION_LEVEL {
        tracing::info!(
            "Calculation for the number of nodes in unconstrained decision tree: {calc}"
        );
    }

    let num_digits = total.to_string().len();
    let display_total: String = if num_digits > DIGITS_LIMIT {
        format!("{total:5.4e}").replace('e', " x 10^")
    } else {
        total.to_string()
    };
    tracing::info!("Total number of nodes in unconstrained decision tree: {display_total}");
    if num_digits > DIGITS_LIMIT && verbosity > SHOW_FULL_DIGITS_LEVEL {
        tracing::info!("The full number of nodes is: {total}");
    }
    tracing::info!("Number of digits in that number: {num_digits}");
    tracing::info!("Average branching factor (versions*builds): {avg_branches}");
    tracing::info!("Number of package builds: {total_builds}");
}
