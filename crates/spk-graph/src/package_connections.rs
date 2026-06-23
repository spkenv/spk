// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::{HashMap, HashSet};

use itertools::Itertools;
pub use petgraph::graph::NodeIndex;
use spk_storage::RepoPackageDependencies;

use crate::PackagesGraph;

/// Collects data about a package's place in among other packages in a
/// repository: its dependencies, the other packages that use it, and
/// its depth (number of levels of dependencies below it) in the
/// repository.
#[derive(Default, Clone)]
pub struct PackageConnections {
    /// The package's node index in the RepoPackageConnections object
    /// it came from.
    pub node_index: NodeIndex,

    /// The display name for this package.
    pub name: String,

    /// The depth of this package within the repo, relative to all the
    /// other packages in the repo, based on the number of levels of
    /// dependencies (at lower depths) required to build or install
    /// this package.
    pub depth: u32,

    /// The direct dependencies of this package, the ones specified in
    /// its specification as build or install requirements.
    pub direct_deps: HashSet<NodeIndex>,

    /// All the dependencies of this package, no matter how far down.
    /// Also known as the transitive dependencies, full dependencies,
    /// or deeper dependencies. This includes the direct dependencies.
    pub all_deps: HashSet<NodeIndex>,

    /// All the other packages that directly depend on this package.
    /// The packages that directly use this one.
    pub direct_used_by: HashSet<NodeIndex>,

    /// All the other packages that depend on this package, no matter
    /// how far above it in the repo. The packages rely on this one
    /// and use it, or use things that use it.
    pub all_used_by: HashSet<NodeIndex>,
}

impl PackageConnections {
    /// Returns true if this package has a direct dependency on the given package.
    pub fn direct_deps_contains(&self, other_package: &PackageConnections) -> bool {
        self.direct_deps.contains(&other_package.node_index)
    }
}

/// A collection of data about all the packages in a repository and
/// the connections between them.
pub struct RepoPackageConnections {
    /// All the package connections data for all the packages in the repo.
    all_packages: HashMap<NodeIndex, PackageConnections>,

    /// A helper to look up a package by name in the connections data.
    all_names_to_indices: HashMap<String, NodeIndex>,
}

impl RepoPackageConnections {
    /// Get the package connections for the given package name, if any.
    pub fn get(&self, name: &String) -> Option<&PackageConnections> {
        let package_index = self.all_names_to_indices.get(name)?;
        self.all_packages.get(package_index)
    }

    /// Get all the package connections.
    pub fn get_all_packages(&self) -> Vec<PackageConnections> {
        self.all_packages
            .values()
            .sorted_by_key(|data| data.name.clone())
            .cloned()
            .collect()
    }

    /// Get all the dependencies of the given package.
    pub fn get_all_deps(&self, package: &PackageConnections) -> Vec<PackageConnections> {
        package
            .all_deps
            .iter()
            .map(|index| self.all_packages.get(index).expect("Node index should be present in a properly constructed RepoPackageConnections object").clone())
            .sorted_by_key(|data| data.name.clone())
            .collect()
    }

    /// Get the direct dependencies of the given package.
    pub fn get_direct_deps_names(&self, package: &PackageConnections) -> Vec<String> {
        package
            .direct_deps
            .iter()
            .map(|index| self.all_packages[index].name.clone())
            .sorted()
            .collect()
    }

    /// Get all packages that use (are clients of) the given package.
    pub fn get_all_used_by(&self, package: &PackageConnections) -> Vec<PackageConnections> {
        package
            .all_used_by
            .iter()
            .map(|index| self.all_packages.get(index).expect("Node index should be present in a properly constructed RepoPackageConnections object").clone())
            .sorted_by_key(|data| data.name.clone())
            .collect()
    }

    /// Construct a RepoPackageConnections object from the given
    /// repository dependencies data.
    pub fn from_repo_dependencies(
        repo_deps_data: &RepoPackageDependencies,
    ) -> RepoPackageConnections {
        // From a packages graph of all the packages, the depths of
        // each package can be computed.
        let mut package_graph = PackagesGraph::from_repo_deps(repo_deps_data);
        package_graph.compute_acyclic_graph_and_toposort();

        let all_depths = package_graph.depths_from_bottom();

        // Set up a PackageConnections object for each package.
        let mut all_packages: HashMap<NodeIndex, PackageConnections> = HashMap::new();

        for (node_index, depth) in all_depths.iter() {
            let name = package_graph.indices_to_names[node_index].clone();

            let pkg = PackageConnections {
                node_index: *node_index,
                name,
                depth: *depth,
                direct_deps: HashSet::new(),
                all_deps: HashSet::new(),
                direct_used_by: HashSet::new(),
                all_used_by: HashSet::new(),
            };
            all_packages.insert(*node_index, pkg);
        }

        // Update the direct used by and all used by sets for all
        // packages. This uses a topological ordering to walk from top
        // of the graph and a reversed graph to have parents (for used
        // by data) as neighbours.
        let mut reversed_edges_graph = package_graph.acyclic_graph.clone();
        reversed_edges_graph.reverse();

        for node_index in package_graph.topological_sort.iter() {
            let mut parents = reversed_edges_graph.neighbors(*node_index).detach();
            while let Some(parent_node) = parents.next_node(&reversed_edges_graph) {
                let transitive_used_by = if let Some(parent_pkg) = all_packages.get(&parent_node) {
                    parent_pkg.all_used_by.clone()
                } else {
                    HashSet::new()
                };

                let pkg = all_packages.entry(*node_index).or_default();

                pkg.direct_used_by.insert(parent_node);

                pkg.all_used_by.insert(parent_node);
                for tu in transitive_used_by.into_iter() {
                    pkg.all_used_by.insert(tu);
                }
            }
        }

        // Update the direct deps and all deps sets for all packages.
        // This uses a topological ordering reversed to walk from
        // bottom of the graph and a normal graph to keep the children
        // as neighbors.
        for node_index in package_graph.topological_sort.iter().rev() {
            let mut children = package_graph.acyclic_graph.neighbors(*node_index).detach();
            while let Some(child_node) = children.next_node(&package_graph.acyclic_graph) {
                let transitive_deps = if let Some(child_pkg) = all_packages.get(&child_node) {
                    child_pkg.all_deps.clone()
                } else {
                    HashSet::new()
                };

                let pkg = all_packages.entry(*node_index).or_default();

                pkg.direct_deps.insert(child_node);

                pkg.all_deps.insert(child_node);
                for td in transitive_deps.into_iter() {
                    pkg.all_deps.insert(td);
                }
            }
        }

        RepoPackageConnections {
            all_packages,
            all_names_to_indices: package_graph.names_to_indices,
        }
    }
}
