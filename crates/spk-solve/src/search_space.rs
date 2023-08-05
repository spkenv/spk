// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::HashMap;
use std::sync::Arc;

use num_bigint::BigUint;
use spk_schema::ident::PkgRequest;
use spk_schema::name::PkgNameBuf;
use spk_schema::{BuildIdent, Deprecate, Package, Spec, VersionIdent};
use spk_solve_solution::Solution;
use spk_storage::Error::InvalidPackageSpec;
use spk_storage::RepositoryHandle;

use crate::{Error, Result};

// Totals with more that this number of digits will be shown using
// scientific notation
const DIGITS_LIMIT: usize = 10;

// Verbosity levels that show more details:
// This changes size numbers display from "1.04 x 10^5" to "10423"
const SHOW_FULL_DIGITS_LEVEL: u8 = 1;
// This adds calculation detail to output, e.g.
//   9844560*6 python-pyside2 (so far: 69062469) +
//   59067360*3 python-pytest (so far: 246264549) + ...
const SHOW_FULL_CALCULATION_LEVEL: u8 = 2;

/// Shows a report on search space sizes and related stats based on
/// the given requests, their solution, and packages in the repos.
pub async fn show_search_space_stats(
    initial_requests: &[String],
    solution: &Solution,
    repos: &[Arc<RepositoryHandle>],
    verbosity: u8,
) -> Result<()> {
    // The names of all packages in the solution are needed to gather
    // data on all their versions and builds. The order the packages
    // were resolved must be kept the same as the order the solver
    // found them (resolved them) during its search for the search
    // space size calculations to be correct.
    let solution_packages = solution
        .items()
        .map(|solved_req| solved_req.spec.ident().clone())
        .collect::<Vec<BuildIdent>>();
    let names = solution_packages.iter().map(|p| p.name().into()).collect();

    let data = get_package_version_build_states(&names, repos).await?;

    // The package requests that constrained (made) the solution are
    // needed to restrict and refine some of the search space sub-reports.
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

/// Returns a list of all the builds for all the versions of the given
/// list of package names.
async fn get_package_version_build_states(
    package_names: &Vec<PkgNameBuf>,
    repos: &[Arc<RepositoryHandle>],
) -> Result<Vec<Arc<Spec>>> {
    let mut data: Vec<Arc<Spec>> = Vec::new();

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
                    let spec = match repo.read_package(&build).await {
                        Ok(s) => s,
                        Err(InvalidPackageSpec(ident, message)) => {
                            // Ignore invalid package spec errors for
                            // the purposes of getting all the valid
                            // package version builds.
                            tracing::warn!("{}", InvalidPackageSpec(ident, message).to_string());
                            continue;
                        }
                        Err(err) => return Err(Error::String(err.to_string())),
                    };
                    data.push(spec);
                }
            }
        }
    }

    Ok(data)
}

/// This produces and outputs 4 sets of search space stats based on
/// the given request, resolve order of the packages, and package
/// build data. The sets are:
/// 1) if all builds without any restrictions were examined
/// 2) if all non-deprecated, non-src builds were examined
/// 3) if all non-deprecated, non-src, non-embedded builds were examined
/// 4) if only non-deprecated, non-src, non-embedded build that also fall within
///    the version ranges of the requests were examined.
fn display_search_space_stats(
    initial_requests: &[String],
    packages: &Vec<BuildIdent>,
    extracted_data: &Vec<Arc<Spec>>,
    verbosity: u8,
    requests: &HashMap<PkgNameBuf, PkgRequest>,
) {
    let mut total_counters: HashMap<PkgNameBuf, u64> = HashMap::new();
    let mut counters: HashMap<PkgNameBuf, u64> = HashMap::new();
    let mut without_embedded_counters: HashMap<PkgNameBuf, u64> = HashMap::new();
    let mut version_range_counters: HashMap<PkgNameBuf, u64> = HashMap::new();

    for spec in extracted_data {
        let ident = spec.ident();
        let pkg_name = ident.name();

        // All builds
        let counter = total_counters.entry(pkg_name.into()).or_insert(0);
        *counter += 1;

        // Active (i.e. non-deprecated), non-src builds
        if !spec.is_deprecated() && !ident.is_source() {
            let counter = counters.entry(pkg_name.into()).or_insert(0);
            *counter += 1;
        }

        // Active, non-src, non-embedded builds
        if !spec.is_deprecated() && !ident.is_source() & !ident.is_embedded() {
            let counter = without_embedded_counters
                .entry(pkg_name.into())
                .or_insert(0);
            *counter += 1;
        }

        // Active, non-src, non-embedded builds, version range
        // matching (so only those that could fall within the
        // request's version range)
        if !spec.is_deprecated() && !ident.is_source() & !ident.is_embedded() {
            if let Some(request) = requests.get(pkg_name) {
                if request.is_satisfied_by(spec).is_ok() {
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

/// This calculates and outputs estimates of the full decision tree's
/// node count based on the given package data. These numbers are
/// "extreme unlikely to happen" upper limits because they do not
/// account for all the dependency subsets, changes, reordering,
/// compat ranges, or per-version expansions of the solver. However,
/// because they represent the number of nodes that would have to be
/// expanded and verified if the entire search tree was visited by a
/// brute force search, they are useful for showing the impact of
/// deprecation and more restrictive requests on the search.
fn show_total_stats(
    counters: &HashMap<PkgNameBuf, u64>,
    packages: &Vec<BuildIdent>,
    verbosity: u8,
) {
    let mut calc: String = "1".to_string();
    let mut total = BigUint::from(1u32);
    let mut total_builds = BigUint::default();
    let mut avg_branches = BigUint::default();
    let mut previous_num_nodes = BigUint::from(1u32);

    for pkg in packages {
        let name = pkg.name();
        if let Some(number) = counters.get(name) {
            let number = *number;
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
    use num_format::ToFormattedString;
    let display_total = total.to_formatted_string(&num_format::Locale::en);
    tracing::info!("Total number of nodes in unconstrained decision tree: {display_total}");
    if num_digits > DIGITS_LIMIT && verbosity > SHOW_FULL_DIGITS_LEVEL {
        tracing::info!("The full number of nodes is: {total}");
    }
    tracing::info!("Number of digits in that number: {num_digits}");
    tracing::info!("Average branching factor (versions*builds): {avg_branches}");
    tracing::info!("Number of package builds: {total_builds}");
}
