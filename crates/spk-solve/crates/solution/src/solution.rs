// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::{
    collections::{HashMap, HashSet},
    fmt::Write,
    iter::FromIterator,
    sync::Arc,
};

use spk_schema::foundation::format::{
    FormatChangeOptions, FormatOptionMap, FormatRequest, FormatSolution,
};
use spk_schema::foundation::ident_component::Component;
use spk_schema::foundation::name::PkgNameBuf;
use spk_schema::foundation::option_map::OptionMap;
use spk_schema::foundation::spec_ops::{Named, PackageOps, Versioned};
use spk_schema::foundation::version::VERSION_SEP;
use spk_schema::ident::{Ident, PkgRequest, RequestedBy};
use spk_schema::{BuildEnv, Package, Spec, SpecRecipe};
use spk_storage::RepositoryHandle;

use crate::{Error, Result};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PackageSource {
    Repository {
        /// the actual repository that this package was loaded from
        repo: Arc<RepositoryHandle>,
        /// the components that can be used for this package from the repository
        components: HashMap<Component, spfs::encoding::Digest>,
    },
    /// The package needs to be build from the given recipe.
    BuildFromSource {
        /// The recipe that this package is to be built from.
        recipe: Arc<SpecRecipe>,
    },
    /// The package was embedded in another.
    Embedded, // TODO: should this reference the source? (it makes the graph code uglier)
}

impl PackageSource {
    pub fn is_build_from_source(&self) -> bool {
        matches!(self, Self::BuildFromSource { .. })
    }

    pub async fn read_recipe(&self, ident: &Ident) -> Result<Arc<SpecRecipe>> {
        match self {
            PackageSource::BuildFromSource { recipe } => Ok(Arc::clone(recipe)),
            PackageSource::Repository { repo, .. } => Ok(repo.read_recipe(ident).await?),
            PackageSource::Embedded => {
                // TODO: what are the implications of this?
                Err(Error::String("Embedded package has no recipe".into()))
            }
        }
    }
}

impl Ord for PackageSource {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        use std::cmp::Ordering;
        use PackageSource::*;
        match (self, other) {
            (this @ Repository { .. }, other @ Repository { .. }) => this.cmp(other),
            (Repository { .. }, BuildFromSource { .. } | Embedded) => Ordering::Less,
            (BuildFromSource { .. } | Embedded, Repository { .. }) => Ordering::Greater,
            (Embedded, Embedded) => Ordering::Equal,
            (Embedded, BuildFromSource { .. }) => Ordering::Less,
            (BuildFromSource { .. }, Embedded) => Ordering::Greater,
            (BuildFromSource { recipe: this }, BuildFromSource { recipe: other }) => {
                this.cmp(other)
            }
        }
    }
}

impl PartialOrd for PackageSource {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// Represents a package request that has been resolved.
pub struct SolvedRequest {
    pub request: PkgRequest,
    pub spec: Arc<Spec>,
    pub source: PackageSource,
}

impl SolvedRequest {
    pub fn is_source_build(&self) -> bool {
        matches!(self.source, PackageSource::BuildFromSource { .. })
    }
}

/// Represents a set of resolved packages.
#[derive(Clone, Debug, Default)]
pub struct Solution {
    options: OptionMap,
    resolved: HashMap<PkgRequest, (Arc<Spec>, PackageSource)>,
    by_name: HashMap<PkgNameBuf, Arc<Spec>>,
    insertion_order: HashMap<PkgRequest, usize>,
}

impl Solution {
    pub fn new(options: Option<OptionMap>) -> Self {
        Self {
            options: options.unwrap_or_default(),
            resolved: HashMap::default(),
            by_name: HashMap::default(),
            insertion_order: HashMap::default(),
        }
    }

    pub fn items(&self) -> Vec<SolvedRequest> {
        let mut items = self
            .resolved
            .clone()
            .into_iter()
            .map(|(request, (spec, source))| SolvedRequest {
                request,
                spec,
                source,
            })
            .collect::<Vec<_>>();
        // Test suite expects these items to be returned in original insertion order.
        items.sort_by_key(|sr| self.insertion_order.get(&sr.request).unwrap());
        items
    }

    pub fn get<S: AsRef<str>>(&self, name: S) -> Option<SolvedRequest> {
        for (request, (spec, source)) in &self.resolved {
            if request.pkg.name.as_str() == name.as_ref() {
                return Some(SolvedRequest {
                    request: request.clone(),
                    spec: spec.clone(),
                    source: source.clone(),
                });
            }
        }
        None
    }

    pub fn options(&self) -> OptionMap {
        self.options.clone()
    }

    /// The number of packages in this solution
    #[inline]
    pub fn len(&self) -> usize {
        self.resolved.len()
    }

    /// The number of packages in this solution
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.resolved.is_empty()
    }

    /// Add a resolved request to this solution
    pub fn add(&mut self, request: &PkgRequest, package: Arc<Spec>, source: PackageSource) {
        if self
            .resolved
            .insert(request.clone(), (package.clone(), source))
            .is_none()
        {
            self.insertion_order
                .insert(request.clone(), self.insertion_order.len());
        }
        self.by_name.insert(request.pkg.name.clone(), package);
    }

    /// Return the set of repositories in this solution.
    pub fn repositories(&self) -> Vec<Arc<RepositoryHandle>> {
        let mut seen = HashSet::new();
        let mut repos = Vec::new();
        for (_, source) in self.resolved.values() {
            if let PackageSource::Repository { repo, .. } = source {
                let addr = repo.address();
                if seen.contains(addr) {
                    continue;
                }
                repos.push(repo.clone());
                seen.insert(addr);
            }
        }
        repos
    }

    /// Return the data of this solution as environment variables.
    ///
    /// If base is given, also clean any existing, conflicting values.
    pub fn to_environment<V>(&self, base: Option<V>) -> HashMap<String, String>
    where
        V: IntoIterator<Item = (String, String)>,
    {
        let mut out = base
            .map(IntoIterator::into_iter)
            .map(HashMap::from_iter)
            .unwrap_or_default();

        out.retain(|name, _| !name.starts_with("SPK_PKG_"));

        out.insert("SPK_ACTIVE_PREFIX".to_owned(), "/spfs".to_owned());
        for (_request, (spec, _source)) in self.resolved.iter() {
            out.insert(format!("SPK_PKG_{}", spec.name()), spec.ident().to_string());
            out.insert(
                format!("SPK_PKG_{}_VERSION", spec.name()),
                spec.version().to_string(),
            );
            out.insert(
                format!("SPK_PKG_{}_BUILD", spec.name()),
                spec.ident()
                    .build
                    .as_ref()
                    .map(|b| b.to_string())
                    .unwrap_or_else(|| "None".to_owned()),
            );
            out.insert(
                format!("SPK_PKG_{}_VERSION_MAJOR", spec.name()),
                spec.version().major().to_string(),
            );
            out.insert(
                format!("SPK_PKG_{}_VERSION_MINOR", spec.name()),
                spec.version().minor().to_string(),
            );
            out.insert(
                format!("SPK_PKG_{}_VERSION_PATCH", spec.name()),
                spec.version().patch().to_string(),
            );
            out.insert(
                format!("SPK_PKG_{}_VERSION_BASE", spec.name()),
                spec.version()
                    .parts
                    .iter()
                    .map(|v| v.to_string())
                    .collect::<Vec<String>>()
                    .join(VERSION_SEP),
            );
        }

        out.extend(self.options.to_environment().into_iter());
        out
    }
}

impl BuildEnv for Solution {
    type Package = Arc<Spec>;

    fn build_env(&self) -> Vec<Self::Package> {
        self.resolved
            .iter()
            .map(|(_, (spec, _))| spec)
            .cloned()
            .collect::<Vec<_>>()
    }
}

impl FormatSolution for Solution {
    fn format_solution(&self, verbosity: u32) -> String {
        if self.is_empty() {
            return "Nothing Installed".to_string();
        }

        let mut out = "Installed Packages:\n".to_string();

        let required_items = self.items();
        let number_of_packages = required_items.len();
        for req in required_items {
            let mut installed =
                PkgRequest::from_ident(req.spec.ident().clone(), RequestedBy::DoesNotMatter);

            if let PackageSource::Repository { components, .. } = req.source {
                let mut installed_components = req.request.pkg.components.clone();
                if installed_components.remove(&Component::All) {
                    installed_components.extend(components.keys().cloned());
                }
                installed.pkg.components = installed_components;
            }

            // Pass zero verbosity to format_request() to stop it
            // outputting the internal details here.
            let _ = write!(
                out,
                "  {}",
                installed.format_request(&None, req.spec.name(), &FormatChangeOptions::default())
            );
            if verbosity > 0 {
                // Get all the things that requested this request
                let requested_by: Vec<String> = req
                    .request
                    .get_requesters()
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<String>>();
                let _ = write!(out, " (required by {}) ", requested_by.join(", "));

                if verbosity > 1 {
                    // Show the options for this request (build)
                    let options = req.spec.option_values();
                    out.push(' ');
                    out.push_str(&options.format_option_map());
                }
            }
            out.push('\n');
        }
        let _ = write!(out, " Number of Packages: {}", number_of_packages);
        out
    }
}
