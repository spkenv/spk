// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use futures::TryStreamExt;
use serde::{Deserialize, Serialize};
use spk_schema::foundation::name::PkgNameBuf;
use spk_schema::foundation::option_map::OptionMap;
use spk_schema::ident::{RangeIdent, RequestWithOptions};
use spk_schema::option_map::OptFilter;
use spk_schema::{BuildIdent, OptionValues, Package, PinnedRequest};

use crate::{Error, RepoWalkerBuilder, RepoWalkerItem, RepositoryHandle, Result};

/// A set of dependencies of a package
type DependencySet = HashSet<PkgNameBuf>;

/// An options map used for filtering package connections
type FilteringOptionsMap = OptionMap;

// TODO: doesn't distinguish 'if already present' dependencies from
// the others yet.
/// The dependencies of a package, based on some options used to filter them
#[derive(Default, Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct PackageDependencies {
    /// The dependencies of a package
    pub deps: DependencySet,

    /// The build options of the build the dependencies came from
    pub options: FilteringOptionsMap,
}

/// Mapping of package names to all their dependencies
pub type PackagesDependenciesMap = HashMap<PkgNameBuf, PackageDependencies>;

/// Set of versions, version ranges that are in use by another package.
type UsingVersionSet = HashSet<RangeIdent>;

/// Mapping of package names to the set of versions, of another package, each uses
pub type UsedByMap = HashMap<PkgNameBuf, UsingVersionSet>;

/// Mapping of package names to the other packages it is used by
/// (and what versions they use/rely on), e.g.:
/// python -(used by)-> pytest -> 1.2.2
///                            -> 2.3.4
///                            -> 2.3.5
type PackagesUsedByMap = HashMap<PkgNameBuf, UsedByMap>;

/// Dependencies and "used by" information on all the packages in a
/// repository. This is a raw collation separated by spec stage.
#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct RepoPackageDependencies {
    /// The builds read to make this dataset, for debugging
    builds: Vec<BuildIdent>,

    /// The install requirements of each package
    pub packages_install_deps: PackagesDependenciesMap,

    /// The packages that call/use/install requirement depends on the
    /// key packages
    pub used_by_install: PackagesUsedByMap,

    /// The builds requirements of each package
    pub packages_build_deps: PackagesDependenciesMap,

    /// The builds that use (requires) each package
    pub used_by_build: PackagesUsedByMap,
}

impl RepoPackageDependencies {
    /// Reads in one example non-deprecated build spec, of each
    /// package in the given repos, and uses them to construct a
    /// RepoPackageDependencies object. The data in a single example
    /// build is not enough to product a complete picture of the all
    /// dependencies between packages, but it is enough to produce a
    /// broad picture of the relationships.
    pub async fn from_repos(
        repos: &Vec<(String, RepositoryHandle)>,
        options_filters: &Option<Vec<OptFilter>>,
    ) -> Result<RepoPackageDependencies> {
        let mut read_in_builds = Vec::new();

        let mut used_by_install: PackagesUsedByMap = HashMap::new();
        let mut packages_install_deps: PackagesDependenciesMap = HashMap::new();
        let mut used_by_build: PackagesUsedByMap = HashMap::new();
        let mut packages_build_deps: PackagesDependenciesMap = HashMap::new();

        let mut got_a_version = HashSet::new();
        let mut spec_count = 0;

        tracing::info!(
            "Gathering dependency data {}...",
            if options_filters.is_some() {
                "with options filters "
            } else {
                ""
            }
        );
        let start = Instant::now();

        let mut repo_walker_builder = RepoWalkerBuilder::new(repos);
        let repo_walker = repo_walker_builder
            .with_report_on_versions(true)
            .with_report_on_builds(true)
            .with_report_src_builds(false)
            .with_report_deprecated_builds(false)
            .with_report_embedded_builds(false)
            .with_build_options_matching(options_filters.clone())
            .with_sort_objects(true)
            .with_continue_on_error(true)
            .with_end_of_markers(true)
            .build();

        let mut traversal = repo_walker.walk();
        while let Some(item) = match traversal.try_next().await {
            Ok(i) => i,
            Err(err) => return Err(Error::String(format!("{err}"))),
        } {
            match item {
                RepoWalkerItem::EndOfVersion(version) => {
                    // Mark that we have processed all the builds of
                    // the highest version of this package.
                    let package = version.ident.name().to_owned();
                    got_a_version.insert(package);
                }

                RepoWalkerItem::Build(build) => {
                    let ident = build.spec.ident();
                    let package = ident.name();
                    let build_spec = build.spec.clone();

                    // Once have processed all the builds for the first
                    // (highest) version to be walked, no more builds of
                    // this package need to be processed.
                    if got_a_version.contains(package) {
                        continue;
                    }

                    spec_count += 1;

                    // Install requirements
                    let mut var_requirements = Vec::new();
                    let requirements = build
                        .spec
                        .runtime_requirements()
                        .iter()
                        .filter_map(|r| match r {
                            RequestWithOptions::Pkg(p) => Some(p.pkg.clone()),
                            RequestWithOptions::Var(v) => {
                                var_requirements.push((v.var.clone(), v.value.to_string()));
                                None
                            }
                        })
                        .collect::<Vec<RangeIdent>>();

                    // Embedded install requirements are ignored because
                    // they don't add a relationship between this package
                    // and another non-embedded package.

                    // Store requirements as dependencies of the current package
                    let entry = packages_install_deps.entry(package.into()).or_default();
                    entry.deps.extend(
                        requirements
                            .iter()
                            .map(|i| i.name.clone())
                            .collect::<Vec<PkgNameBuf>>(),
                    );

                    // Store all the options so can filter on them later
                    entry.options = build_spec.option_values();

                    // Build requirements
                    let mut build_options = Vec::new();
                    let build_requirements = build_spec
                        .get_build_requirements()?
                        .into_owned()
                        .iter()
                        .filter_map(|r| match r {
                            PinnedRequest::Pkg(p) => Some(p.pkg.clone()),
                            PinnedRequest::Var(v) => {
                                build_options.push((v.var.clone(), v.value.to_string()));
                                None
                            }
                        })
                        .collect::<Vec<RangeIdent>>();

                    // Adding to package_build_deps
                    let entry = packages_build_deps.entry(package.into()).or_default();
                    entry.deps.extend(
                        build_requirements
                            .iter()
                            .map(|i| i.name.clone())
                            .collect::<Vec<PkgNameBuf>>(),
                    );
                    entry.options = OptionMap::from_iter(build_options);

                    // The install requirements are also what uses each
                    // (other) package when inverted - the used_by info.
                    //
                    // Put the package into used by data as one that uses
                    // each of its requirements.
                    for requirement in requirements {
                        let using_entry =
                            used_by_install.entry(requirement.name.clone()).or_default();
                        let versions_entry = using_entry.entry(ident.name().into()).or_default();
                        versions_entry.insert(requirement);
                    }

                    // The build requirements are also what uses each
                    // (other) package when inverted - the used_by_builds info.
                    //
                    // Put the package into used by data as one that uses
                    // each of its requirements.
                    for requirement in build_requirements {
                        let using_entry =
                            used_by_build.entry(requirement.name.clone()).or_default();
                        let versions_entry = using_entry.entry(ident.name().into()).or_default();
                        versions_entry.insert(requirement);
                    }

                    read_in_builds.push(ident.clone())
                }

                _ => {}
            }
        }

        tracing::debug!(
            "Took {} seconds to read in {spec_count} specs for {} packages (builds/package: {} average)",
            start.elapsed().as_secs_f64(),
            used_by_install.len(),
            if used_by_install.is_empty() {
                0
            } else {
                spec_count / used_by_install.len()
            }
        );

        let data = RepoPackageDependencies {
            builds: read_in_builds,
            packages_install_deps,
            used_by_install,
            packages_build_deps,
            used_by_build,
        };

        Ok(data)
    }
}
