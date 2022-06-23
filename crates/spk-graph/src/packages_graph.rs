// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Display;

use itertools::Itertools;
use once_cell::sync::Lazy;
use petgraph::algo;
pub use petgraph::graph::{EdgeIndex, NodeIndex};
use petgraph::visit::EdgeRef;
use spk_schema::AnyIdent;
use spk_schema::ident::{RequestedBy, parse_ident};
use spk_storage::RepoPackageDependencies;

/// The name of the spk stdfs package.
const STDFS_SPK_PACKAGE: &str = "stdfs";

/// List of package names to ignore when constructing packages
/// graphs. The list can be configured via the spk config file, but
/// stdfs is always added to it.
static PACKAGES_TO_IGNORE: Lazy<Vec<String>> = Lazy::new(|| {
    let mut ignore_list = match spk_config::get_config() {
        Ok(c) => c.graph.ignore.clone(),
        Err(_err) => Vec::new(),
    };
    // Can always ignore the stdfs package for packages graphs because
    // it is ubiquitous and becomes noise in the calculations.
    ignore_list.push(STDFS_SPK_PACKAGE.to_string());
    ignore_list
});

/// Returns true if the given package name is one to ignore and filter
/// out when constructing a packages graph. The packages that can be
/// ignored are combination of stdfs, any other packages configured in
/// the ignore list, and the platforms so any package ending in "-platform".
fn package_to_ignore<S: AsRef<str>>(package: &S) -> bool {
    PACKAGES_TO_IGNORE.contains(&package.as_ref().to_string())
        || package.as_ref().ends_with("-platform")
}

/// The possible identifiers for a package graph node. Most will be
/// idents, but some are better represented as strings (e.g. command
/// line requests, unresolved requests).
#[derive(Debug, Hash, Eq, PartialEq, Ord, PartialOrd, Clone)]
pub enum GraphNodeId {
    Ident(AnyIdent),
    String(String),
}

impl GraphNodeId {
    pub fn name(&self) -> String {
        match self {
            GraphNodeId::Ident(ident) => ident.name().to_string(),
            GraphNodeId::String(name) => name.to_string(),
        }
    }

    pub fn version(&self) -> String {
        match self {
            GraphNodeId::Ident(ident) => ident.version().to_string(),
            GraphNodeId::String(_) => "".to_string(),
        }
    }

    pub fn build(&self) -> String {
        match self {
            GraphNodeId::Ident(ident) => match ident.build() {
                Some(b) => b.to_string(),
                None => "".to_string(),
            },
            GraphNodeId::String(_) => "".to_string(),
        }
    }

    pub fn has_build(&self) -> bool {
        match self {
            GraphNodeId::Ident(ident) => ident.build().is_some(),
            GraphNodeId::String(_) => false,
        }
    }
}

impl Display for GraphNodeId {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            GraphNodeId::Ident(ident) => fmt.write_str(&ident.to_string()),
            GraphNodeId::String(name) => fmt.write_str(name),
        }?;
        Ok(())
    }
}

/// A node in a packages graph.
#[derive(Debug, Hash, Eq, PartialEq, Ord, PartialOrd, Clone)]
pub struct GraphNode {
    /// The name of the node
    pub name: GraphNodeId,
}

/// A graph of packages connected by dependency or client/used by relationships.
#[derive(Default)]
pub struct PackagesGraph {
    /// A mapping of nodes to their parent nodes (packages that requested them).
    pub nodes_to_parents: BTreeMap<GraphNode, HashSet<RequestedBy>>,

    /// Directed acyclic graph based on the nodes to parents data.
    /// Only valid after the
    /// [`PackagesGraph::compute_acyclic_graph_and_toposort`] method
    /// has been called.
    pub(crate) acyclic_graph:
        petgraph::Graph<String, (), petgraph::Directed, petgraph::graph::DefaultIx>,

    /// Topological ordering of the acyclic graph. Only valid after
    /// the [`PackagesGraph::compute_acyclic_graph_and_toposort`]
    /// method has been called.
    pub(crate) topological_sort: Vec<NodeIndex>,

    /// A helper mapping of node names to acyclic graph indices. Only
    /// valid after the
    /// [`PackagesGraph::compute_acyclic_graph_and_toposort`] method
    /// has been called.
    pub(crate) names_to_indices: HashMap<String, NodeIndex>,

    /// A helper mapping of acyclic graph indices to node names. Only
    /// valid after the
    /// [`PackagesGraph::compute_acyclic_graph_and_toposort`] has been
    /// called.
    pub(crate) indices_to_names: HashMap<NodeIndex, String>,
}

impl PackagesGraph {
    /// Construct a PackagesGraph from all the packages in the given
    /// repo package dependencies.
    pub fn from_repo_deps(repo_deps_data: &RepoPackageDependencies) -> PackagesGraph {
        // TODO: make this an option for constructing others kinds of
        // graphs. For now, it is enabled for the inventory command.
        let include_build_deps = true;

        // Assemble the dependencies data this is going to use.
        // Either just install based, or install and build based.
        let mut combined_deps = repo_deps_data.used_by_install.clone();
        if include_build_deps {
            for (pkg_name, used_by) in &repo_deps_data.used_by_build {
                let using_entry = combined_deps.entry(pkg_name.clone()).or_default();
                for (key, version_set) in used_by.iter() {
                    let versions_entry = using_entry.entry(key.clone()).or_default();
                    versions_entry.extend(version_set.clone());
                }
            }
        }

        // Then make the graph based on those dependencies.
        let mut packages_graph: PackagesGraph = Default::default();

        let mut seen = HashSet::new();
        for (package, parents) in combined_deps.iter() {
            if package_to_ignore(package) {
                continue;
            }

            // Sanity check, things without any dependencies entry at
            // all are not packages that were read in. They are
            // dangling references to packages. For example,
            // deprecated packages referenced in other non-deprecated
            // packages. We want to filter these out because we don't
            // have valid information on them.
            if !repo_deps_data.packages_install_deps.contains_key(package) {
                tracing::debug!(
                    "{package} no entry in package_deps, but is 'used by': {}",
                    parents.keys().map(ToString::to_string).join(", ")
                );
                continue;
            }

            // Ensure this package is in the graph along with its
            // requesters.
            seen.insert(package.clone());
            let package_id = parse_ident(package.clone()).unwrap();
            let pkg = GraphNode {
                name: GraphNodeId::Ident(package_id.clone()),
            };

            let parents_set = packages_graph
                .nodes_to_parents
                .entry(pkg.clone())
                .or_default();
            for parent in parents.keys() {
                if package_to_ignore(parent) {
                    continue;
                }

                let parent_id = parse_ident(parent).unwrap();
                let requester = RequestedBy::PackageVersion(parent_id.to_version_ident());
                parents_set.insert(requester.clone());
            }
        }

        // Ensure packages that no other packages use (the top level
        // packages, e.g. tools, not libraries) have been included.
        for package in repo_deps_data.packages_install_deps.keys().sorted() {
            if !seen.contains(package) {
                let package_id = parse_ident(package.clone()).unwrap();
                let pkg = GraphNode {
                    name: GraphNodeId::Ident(package_id.clone()),
                };

                // This will add the package to the graph.
                let _parents_set = packages_graph
                    .nodes_to_parents
                    .entry(pkg.clone())
                    .or_default();

                // This node has no parents, so there's nothing else
                // to add to the set.
                seen.insert(package.clone());
            }
        }

        packages_graph
    }

    /// Return a list of Graph nodes in this graph.
    pub fn nodes(&self) -> Vec<&GraphNode> {
        self.nodes_to_parents.keys().collect()
    }

    /// Calculates and stores an acyclic graph and topological
    /// ordering for the PackagesGraph. This will replace any
    /// previously calculated data. This must be called to compute and
    /// store valid data in the majority of a PackageGraph's fields.
    /// Accessing those fields before calling this will give incorrect
    /// data.
    pub fn compute_acyclic_graph_and_toposort(&mut self) -> &mut Self {
        let mut g =
            petgraph::Graph::<String, (), petgraph::Directed, petgraph::graph::DefaultIx>::new();

        let mut names_to_indices: HashMap<String, NodeIndex> = HashMap::new();

        // Add all the nodes, caching their name and index as it goes.
        let nodes: Vec<String> = self
            .nodes_to_parents
            .keys()
            .map(|k| k.name.to_string())
            .collect();

        for name in nodes.iter() {
            if !names_to_indices.contains_key(name) {
                let nix = g.add_node(name.clone());
                names_to_indices.insert(name.clone(), nix);
            }
        }

        // Add all the edges
        for (node, requesters) in self.nodes_to_parents.iter() {
            let name = node.name.to_string();
            let node_index = names_to_indices.get(&name).unwrap();
            for parent in requesters {
                let parent_name = match parent.ident() {
                    Ok(n) => n.to_string(),
                    Err(e) => {
                        if let RequestedBy::CommandLineRequest(initial_request) = parent {
                            format!("{initial_request}")
                        } else {
                            tracing::warn!(
                                "computing acyclic graph: skipping: {parent} due to {e}"
                            );
                            continue;
                        }
                    }
                };

                if let Some(parent_index) = names_to_indices.get(&parent_name) {
                    g.add_edge(*parent_index, *node_index, ());
                }
            }
        }

        // Make sure the graph is acyclic by removing problematic
        // edges. The topological sorting needs an acyclic graph.
        let feedback_arc_set: Vec<EdgeIndex> =
            petgraph::algo::feedback_arc_set::greedy_feedback_arc_set(&g)
                .map(|e| e.id())
                .collect();

        for edge_index in feedback_arc_set {
            tracing::warn!(
                "Removing edge in f.a.s.: {:?} {}",
                edge_index,
                match g.edge_endpoints(edge_index) {
                    Some((ni1, ni2)) => {
                        format!(
                            " - {} -> {}",
                            g.node_weight(ni1).unwrap(),
                            g.node_weight(ni2).unwrap()
                        )
                    }
                    None => {
                        "error".to_string()
                    }
                }
            );
            // Remove edges in feedback arc set from original graph
            let _e = g.remove_edge(edge_index);
        }

        // Get a topological ordering the nodes in the graph.
        let toposort = match algo::toposort(&g, None) {
            Ok(ts) => ts,
            Err(err) => {
                // Only errors if the graph has cycles in it, which it
                // should not have at this point.
                let nid = err.node_id();
                // Have to do a value search, which is more costly,
                // but it is only done when there's an error.
                for (name, idx) in names_to_indices.iter() {
                    if *idx == nid {
                        tracing::warn!("toposort failed?: {err:?}, {nid:?} = {name}");
                    }
                }
                Vec::new()
            }
        };

        // Generate a node index to name cache.
        let indices_to_names = names_to_indices
            .iter()
            .map(|(n, i)| (*i, n.clone()))
            .collect::<HashMap<NodeIndex, String>>();

        // Store all the computed data for future use
        self.acyclic_graph = g;
        self.topological_sort = toposort;
        self.names_to_indices = names_to_indices;
        self.indices_to_names = indices_to_names;
        self
    }

    /// Calculates the depth of each node from bottom nodes (0) up to
    /// the top/root (n), based on the depth of each nodes dependencies.
    pub fn depths_from_bottom(&self) -> BTreeMap<NodeIndex<u32>, u32> {
        let mut node_depths: BTreeMap<NodeIndex<u32>, u32> = BTreeMap::new();

        // Reverses the topological sorting to start at bottom nodes
        // and check children of the nodes as it goes.
        for n in self.topological_sort.iter().rev() {
            if let Some(name) = self.indices_to_names.get(n)
                && package_to_ignore(name)
            {
                continue;
            }

            let mut max_value = 0;
            let mut children = self.acyclic_graph.neighbors(*n).detach();
            while let Some(node) = children.next_node(&self.acyclic_graph) {
                if let Some(name) = self.indices_to_names.get(&node)
                    && package_to_ignore(name)
                {
                    continue;
                }

                max_value = std::cmp::max(max_value, *node_depths.get(&node).unwrap());
            }
            let depth = 1 + max_value;
            node_depths.insert(*n, depth);
        }

        node_depths
    }
}
