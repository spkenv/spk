// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use crate::{
    api::{self, Ident},
    prelude::*,
    storage, Error, Result,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PackageSource {
    Repository {
        /// the actual repository that this package was loaded from
        repo: Arc<storage::RepositoryHandle>,
        /// the components that can be used for this package from the repository
        components: HashMap<api::Component, spfs::encoding::Digest>,
    },
    /// The package needs to be build from the given recipe.
    BuildFromSource {
        /// The recipe that this package is to be built from.
        recipe: Arc<api::SpecRecipe>,
    },
    /// The package was embedded in another.
    Embedded, // TODO: should this reference the source? (it makes the graph code uglier)
}

impl PackageSource {
    pub fn is_build_from_source(&self) -> bool {
        matches!(self, Self::BuildFromSource { .. })
    }

    pub async fn read_recipe(&self, ident: &Ident) -> Result<Arc<api::SpecRecipe>> {
        match self {
            PackageSource::BuildFromSource { recipe } => Ok(Arc::clone(recipe)),
            PackageSource::Repository { repo, .. } => repo.read_recipe(ident).await,
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
    pub request: api::PkgRequest,
    pub spec: Arc<api::Spec>,
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
    options: api::OptionMap,
    resolved: HashMap<api::PkgRequest, (Arc<api::Spec>, PackageSource)>,
    by_name: HashMap<api::PkgNameBuf, Arc<api::Spec>>,
    insertion_order: HashMap<api::PkgRequest, usize>,
}

impl Solution {
    pub fn new(options: Option<api::OptionMap>) -> Self {
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

    pub fn options(&self) -> api::OptionMap {
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
    pub fn add(
        &mut self,
        request: &api::PkgRequest,
        package: Arc<api::Spec>,
        source: PackageSource,
    ) {
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
    pub fn repositories(&self) -> Vec<Arc<storage::RepositoryHandle>> {
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
                    .join(api::VERSION_SEP),
            );
        }

        out.extend(self.options.to_environment().into_iter());
        out
    }
}
