// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use crate::{
    api::{self, Ident},
    storage, Result,
};

#[derive(Clone, Debug)]
pub enum PackageSource {
    Repository {
        /// the actual repository that this package was loaded from
        repo: Arc<storage::RepositoryHandle>,
        /// the components that can be used for this package from the repository
        components: HashMap<api::Component, spfs::encoding::Digest>,
    },
    // A package comes from another spec if it is either an embedded
    // package or represents a package to be built from source. In the
    // latter case, this spec is the original source spec that should
    // be used as the basis for the package build.
    Spec(Arc<api::Spec>),
}

impl PackageSource {
    pub async fn read_spec(&self, ident: &Ident) -> Result<api::Spec> {
        match self {
            PackageSource::Spec(s) => Ok((**s).clone()),
            PackageSource::Repository { repo, .. } => repo.read_spec(ident).await,
        }
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
        match &self.source {
            PackageSource::Repository { .. } => false,
            PackageSource::Spec(spec) => spec.pkg == self.spec.pkg.with_build(None),
        }
    }
}

/// Represents a set of resolved packages.
#[derive(Clone, Debug)]
pub struct Solution {
    options: api::OptionMap,
    resolved: HashMap<api::PkgRequest, (Arc<api::Spec>, PackageSource)>,
    by_name: HashMap<api::PkgName, Arc<api::Spec>>,
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
}

impl Solution {
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
                if seen.contains(&addr) {
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
        use std::iter::FromIterator;
        let mut out = base
            .map(IntoIterator::into_iter)
            .map(HashMap::from_iter)
            .unwrap_or_default();

        out.retain(|name, _| !name.starts_with("SPK_PKG_"));

        out.insert("SPK_ACTIVE_PREFIX".to_owned(), "/spfs".to_owned());
        for (_request, (spec, _source)) in self.resolved.iter() {
            out.insert(format!("SPK_PKG_{}", spec.pkg.name), spec.pkg.to_string());
            out.insert(
                format!("SPK_PKG_{}_VERSION", spec.pkg.name),
                spec.pkg.version.to_string(),
            );
            out.insert(
                format!("SPK_PKG_{}_BUILD", spec.pkg.name),
                spec.pkg
                    .build
                    .as_ref()
                    .map(|b| b.to_string())
                    .unwrap_or_else(|| "None".to_owned()),
            );
            out.insert(
                format!("SPK_PKG_{}_VERSION_MAJOR", spec.pkg.name),
                spec.pkg.version.major().to_string(),
            );
            out.insert(
                format!("SPK_PKG_{}_VERSION_MINOR", spec.pkg.name),
                spec.pkg.version.minor().to_string(),
            );
            out.insert(
                format!("SPK_PKG_{}_VERSION_PATCH", spec.pkg.name),
                spec.pkg.version.patch().to_string(),
            );
            out.insert(
                format!("SPK_PKG_{}_VERSION_BASE", spec.pkg.name),
                spec.pkg
                    .version
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
