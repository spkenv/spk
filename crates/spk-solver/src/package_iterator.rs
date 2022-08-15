// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use dyn_clone::DynClone;
use once_cell::sync::Lazy;
use spk_ident::Ident;
use spk_name::{OptNameBuf, PkgNameBuf, RepositoryNameBuf};
use spk_option_map::OptionMap;
use spk_spec::{Package, Spec, SpecRecipe};
use spk_spec_ops::PackageOps;
use spk_storage::RepositoryHandle;
use spk_version::Version;
use std::ffi::OsString;
use std::time::{Duration, Instant};
use std::{
    collections::{HashMap, VecDeque},
    convert::TryFrom,
    pin::Pin,
    sync::Arc,
};

use super::solution::PackageSource;
use crate::build_key::BuildKey;
use crate::{Error, Result};

#[cfg(test)]
#[path = "./package_iterator_test.rs"]
mod package_iterator_test;

/// Allows control of the order option names are using in build key
/// generation. Names in this list will be put at the front of the
/// list of option names used to generate keys for ordering builds
/// during a solve step. This does not have contain all the possible
/// option names. But names not in this list will come after these
/// ones, in alphabetical order so their relative ordering is
/// consistent across packages.
//
// TODO: add the default value to a config file, once spk has one
static BUILD_KEY_NAME_ORDER: Lazy<Vec<OptNameBuf>> = Lazy::new(|| {
    std::env::var_os("SPK_BUILD_OPTION_KEY_ORDER")
        .unwrap_or_else(|| OsString::from("gcc,python"))
        .to_string_lossy()
        .to_string()
        .split(',')
        .map(|n| OptNameBuf::try_from(n).map_err(crate::Error::from))
        .filter_map(Result::ok)
        .collect()
});

type BuildWithRepos = HashMap<RepositoryNameBuf, (Arc<Spec>, PackageSource)>;

#[async_trait::async_trait]
pub trait BuildIterator: DynClone + Send + Sync + std::fmt::Debug {
    fn is_empty(&self) -> bool;
    fn is_sorted_build_iterator(&self) -> bool {
        false
    }
    async fn next(&mut self) -> crate::Result<Option<BuildWithRepos>>;
    async fn recipe(&self) -> Option<Arc<SpecRecipe>>;
    fn len(&self) -> usize;
}

/// A tracing target for additional build sorting output: times,
/// building blocks, and keys.
const BUILD_SORT_TARGET: &str = "build_sort";

dyn_clone::clone_trait_object!(BuildIterator);

type PackageIteratorItem = (Ident, Arc<tokio::sync::Mutex<dyn BuildIterator + Send>>);

#[async_trait::async_trait]
pub trait PackageIterator: Send + Sync + std::fmt::Debug {
    async fn async_clone(&self) -> Box<dyn PackageIterator + Send>;

    fn next<'a>(
        &'a mut self,
    ) -> Pin<
        Box<dyn futures::Future<Output = crate::Result<Option<PackageIteratorItem>>> + Send + 'a>,
    >;

    /// Replaces the internal build iterator for version with the given one.
    fn set_builds(
        &mut self,
        version: &Version,
        builds: Arc<tokio::sync::Mutex<dyn BuildIterator + Send>>,
    );
}

#[derive(Clone, Debug)]
struct VersionIterator {
    versions: VecDeque<Arc<Version>>,
}

impl Iterator for VersionIterator {
    type Item = Arc<Version>;

    fn next(&mut self) -> Option<Self::Item> {
        self.versions.pop_front()
    }
}

impl VersionIterator {
    fn new(versions: VecDeque<Arc<Version>>) -> Self {
        VersionIterator { versions }
    }
}

type RepositoryByNameByVersion =
    HashMap<Arc<Version>, HashMap<RepositoryNameBuf, Arc<RepositoryHandle>>>;

/// A stateful cursor yielding package builds from a set of repositories.
#[derive(Debug)]
pub struct RepositoryPackageIterator {
    pub package_name: PkgNameBuf,
    pub repos: Vec<Arc<RepositoryHandle>>,
    versions: Option<VersionIterator>,
    version_map: RepositoryByNameByVersion,
    builds_map: HashMap<Version, Arc<tokio::sync::Mutex<dyn BuildIterator + Send>>>,
    active_version: Option<Arc<Version>>,
}

#[async_trait::async_trait]
impl PackageIterator for RepositoryPackageIterator {
    /// Create a copy of this iterator, with the cursor at the same point.
    async fn async_clone(&self) -> Box<dyn PackageIterator + Send> {
        let version_map = if self.versions.is_none() {
            match self.build_version_map().await {
                Ok(version_map) => version_map,
                Err(Error::SpkValidatorsError(spk_validators::Error::PackageNotFoundError(_))) => {
                    return Box::new(RepositoryPackageIterator::new(
                        self.package_name.clone(),
                        self.repos.clone(),
                    ))
                }
                Err(err) => {
                    // we wanted to save the clone from causing this
                    // work to be done twice, but it's not fatal
                    tracing::trace!(
                        "Encountered error cloning RepositoryPackageIterator: {:?}",
                        err
                    );
                    self.version_map.clone()
                }
            }
        } else {
            self.version_map.clone()
        };

        Box::new(RepositoryPackageIterator {
            package_name: self.package_name.clone(),
            repos: self.repos.clone(),
            versions: self.versions.clone(),
            version_map,
            // Python custom clone() doesn't clone the remaining fields
            builds_map: HashMap::default(),
            active_version: None,
        })
    }

    fn next<'a>(
        &'a mut self,
    ) -> Pin<
        Box<dyn futures::Future<Output = crate::Result<Option<PackageIteratorItem>>> + Send + 'a>,
    > {
        Box::pin(async move {
            if self.versions.is_none() {
                self.start().await?
            }

            if self.active_version.is_none() {
                self.active_version = self.versions.as_mut().and_then(|i| i.next());
            }
            let version = if let Some(active_version) = self.active_version.as_ref() {
                active_version
            } else {
                return Ok(None);
            };
            let repos = if let Some(repo) = self.version_map.get(version) {
                repo
            } else {
                return Err(crate::Error::String(
                    "version not found in version_map".to_owned(),
                ));
            };
            let mut pkg = Ident::new(self.package_name.clone());
            pkg.version = (**version).clone();
            if !self.builds_map.contains_key(version) {
                match RepositoryBuildIterator::new(pkg.clone(), repos.clone()).await {
                    Ok(iter) => {
                        self.builds_map
                            .insert((**version).clone(), Arc::new(tokio::sync::Mutex::new(iter)));
                    }
                    Err(
                        err @ Error::SpkStorageError(spk_storage::Error::InvalidPackageSpec(..)),
                    ) => {
                        tracing::warn!("Skipping: {}", err);
                        self.active_version = None;
                        return self.next().await;
                    }
                    Err(err) => return Err(err),
                }
            }
            let builds = self.builds_map.get(version).unwrap();
            if builds.lock().await.is_empty() {
                self.active_version = None;
                return self.next().await;
            }
            Ok(Some((pkg, builds.clone())))
        })
    }

    fn set_builds(
        &mut self,
        version: &Version,
        builds: Arc<tokio::sync::Mutex<dyn BuildIterator + Send>>,
    ) {
        self.builds_map.insert(version.clone(), builds);
    }
}

impl RepositoryPackageIterator {
    pub fn new(package_name: PkgNameBuf, repos: Vec<Arc<RepositoryHandle>>) -> Self {
        RepositoryPackageIterator {
            package_name,
            repos,
            versions: None,
            version_map: HashMap::default(),
            builds_map: HashMap::default(),
            active_version: None,
        }
    }

    async fn build_version_map(&self) -> Result<RepositoryByNameByVersion> {
        let mut version_map: RepositoryByNameByVersion = HashMap::default();
        // Keep track of all the repos that possess this version so it is
        // possible to filter by repo later.
        for repo in self.repos.iter().rev() {
            for version in repo.list_package_versions(&self.package_name).await?.iter() {
                match version_map.get_mut(version) {
                    Some(repos) => {
                        repos.insert(repo.name().to_owned(), Arc::clone(repo));
                    }
                    None => {
                        version_map.insert(
                            Arc::clone(version),
                            HashMap::from([(repo.name().to_owned(), Arc::clone(repo))]),
                        );
                    }
                }
            }
        }

        if version_map.is_empty() {
            return Err(spk_validators::Error::PackageNotFoundError(
                self.package_name.clone().into(),
            )
            .into());
        }

        Ok(version_map)
    }

    async fn start(&mut self) -> Result<()> {
        self.version_map = self.build_version_map().await?;
        let mut versions: Vec<Arc<Version>> =
            self.version_map.keys().into_iter().cloned().collect();
        versions.sort();
        versions.reverse();
        self.versions = Some(VersionIterator::new(versions.into()));
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct RepositoryBuildIterator {
    builds: VecDeque<(Ident, HashMap<RepositoryNameBuf, Arc<RepositoryHandle>>)>,
    recipe: Option<Arc<SpecRecipe>>,
}

#[async_trait::async_trait]
impl BuildIterator for RepositoryBuildIterator {
    fn is_empty(&self) -> bool {
        self.builds.is_empty()
    }

    async fn next(&mut self) -> crate::Result<Option<BuildWithRepos>> {
        let (build, repos) = if let Some(build) = self.builds.pop_front() {
            build
        } else {
            return Ok(None);
        };

        let mut result = HashMap::new();

        for (repo_name, repo) in repos.iter() {
            let spec = match repo.read_package(&build).await {
                Ok(spec) => spec,
                Err(spk_storage::Error::SpkValidatorsError(
                    spk_validators::Error::PackageNotFoundError(_),
                )) => {
                    tracing::warn!("Repository listed build with no spec: {build} from {repo:?}",);
                    // Skip to next build
                    return self.next().await;
                }
                Err(err) => return Err(err.into()),
            };

            let components = match repo.read_components(&build).await {
                Ok(c) => c,
                Err(spk_storage::Error::SpkValidatorsError(
                    spk_validators::Error::PackageNotFoundError(_),
                )) => Default::default(),
                Err(err) => return Err(err.into()),
            };

            if spec.ident().build.is_none() {
                tracing::warn!("Published spec is corrupt (has no associated build), pkg={build}",);
                return self.next().await;
            }

            result.insert(
                repo_name.clone(),
                (
                    spec,
                    PackageSource::Repository {
                        repo: Arc::clone(repo),
                        components,
                    },
                ),
            );
        }

        Ok(Some(result))
    }

    async fn recipe(&self) -> Option<Arc<SpecRecipe>> {
        self.recipe.clone()
    }

    fn len(&self) -> usize {
        self.builds.len()
    }
}

impl RepositoryBuildIterator {
    async fn new(
        pkg: Ident,
        repos: HashMap<RepositoryNameBuf, Arc<RepositoryHandle>>,
    ) -> Result<Self> {
        let mut builds_and_repos: HashMap<
            Ident,
            HashMap<RepositoryNameBuf, Arc<RepositoryHandle>>,
        > = HashMap::new();

        let mut recipe = None;
        for (repo_name, repo) in &repos {
            let builds = repo.list_package_builds(&pkg).await?;
            for build in builds {
                match builds_and_repos.get_mut(&build) {
                    Some(repos) => {
                        repos.insert(repo_name.clone(), Arc::clone(repo));
                    }
                    None => {
                        builds_and_repos.insert(
                            build,
                            HashMap::from([(repo_name.clone(), Arc::clone(repo))]),
                        );
                    }
                }
            }
            if recipe.is_none() {
                recipe = match repo.read_recipe(&pkg).await {
                    Ok(spec) => Some(spec),
                    Err(spk_storage::Error::SpkValidatorsError(
                        spk_validators::Error::PackageNotFoundError(_),
                    )) => None,
                    Err(err) => return Err(err.into()),
                };
            }
        }

        let mut builds = builds_and_repos.into_iter().collect::<Vec<_>>();

        // source packages must come last to ensure that building
        // from source is the last option under normal circumstances
        builds.sort_by_key(|(pkg, _)| !pkg.is_source());

        Ok(RepositoryBuildIterator {
            builds: builds.into(),
            recipe,
        })
    }
}

#[derive(Clone, Debug)]
pub struct EmptyBuildIterator {}

#[async_trait::async_trait]
impl BuildIterator for EmptyBuildIterator {
    fn is_empty(&self) -> bool {
        true
    }

    async fn next(&mut self) -> crate::Result<Option<BuildWithRepos>> {
        Ok(None)
    }

    async fn recipe(&self) -> Option<Arc<SpecRecipe>> {
        None
    }

    fn len(&self) -> usize {
        0
    }
}

impl EmptyBuildIterator {
    pub fn new() -> Self {
        EmptyBuildIterator {}
    }
}

#[derive(Clone, Debug)]
pub struct SortedBuildIterator {
    source: Arc<tokio::sync::Mutex<dyn BuildIterator + Send>>,
    builds: VecDeque<BuildWithRepos>,
}

#[async_trait::async_trait]
impl BuildIterator for SortedBuildIterator {
    fn is_empty(&self) -> bool {
        self.builds.is_empty()
    }

    fn is_sorted_build_iterator(&self) -> bool {
        true
    }

    async fn next(&mut self) -> crate::Result<Option<BuildWithRepos>> {
        Ok(self.builds.pop_front())
    }

    async fn recipe(&self) -> Option<Arc<SpecRecipe>> {
        self.source.lock().await.recipe().await
    }

    fn len(&self) -> usize {
        self.builds.len()
    }
}

/// A helper for working out whether a named option value changes
/// across builds, or is always they same for all binary builds.
/// Options with differing values across builds are worth using
/// (use_it) in a build key to distinguish builds for sorting. Options
/// that don't vary are not worth using in the build key.
struct ChangeCounter {
    pub last: String,
    pub count: u64,
    pub use_it: bool,
}

impl SortedBuildIterator {
    pub async fn new(
        _options: OptionMap,
        source: Arc<tokio::sync::Mutex<dyn BuildIterator + Send>>,
    ) -> Result<Self> {
        // Note: _options is unused in this implementation, it was used
        // in the by_distance sorting implementation
        let mut builds = VecDeque::<BuildWithRepos>::new();
        {
            let mut source_lock = source.lock().await;
            while let Some(item) = source_lock.next().await? {
                builds.push_back(item);
            }
        }

        let mut sbi = SortedBuildIterator { source, builds };

        sbi.sort_by_build_option_values();
        Ok(sbi)
    }

    /// Helper for making BuildKey structures used in the sorting in
    /// sort_by_build_option_values() below
    fn make_option_values_build_key(
        spec: &Spec,
        ordered_names: &Vec<OptNameBuf>,
        build_name_values: &HashMap<Ident, OptionMap>,
    ) -> BuildKey {
        let build_id = spec.ident();
        let empty = OptionMap::default();
        let name_values = match build_name_values.get(build_id) {
            Some(nv) => nv,
            None => &empty,
        };
        BuildKey::new(spec.ident(), ordered_names, name_values)
    }

    /// Sorts builds by keys based on ordered build option names and
    /// differing values in those options
    fn sort_by_build_option_values(&mut self) {
        let start = Instant::now();

        let mut number_non_src_builds: u64 = 0;
        let mut build_name_values: HashMap<Ident, OptionMap> = HashMap::default();
        let mut changes: HashMap<OptNameBuf, ChangeCounter> = HashMap::new();

        for (build, _) in self.builds.iter().flat_map(|hm| hm.values()) {
            // Skip this if it's '/src' build because '/src' builds
            // won't use the build option values in their key, they
            // don't need to be looked at. They have a type of key
            // that always puts them last in the build order.
            if let Some(b) = &build.ident().build {
                if b.is_source() {
                    continue;
                }
            }

            // Count the number of binary builds for later. This will
            // be used to help work out whether a build option has the
            // same value across the builds.
            number_non_src_builds += 1;

            // Get this build's resolved options and store them for
            // later use when generating the build's key object. They
            // won't all be used in the key, but this saves having to
            // regenerate them.
            let options_map = build.option_values();

            // Work out which options will be used in the keys. This
            // is done for all builds before the first key is
            // generated so options with identical values across all
            // builds can be ignored. Only using the options that
            // differ across builds gives shorter, more distinct, keys.
            //
            // The build option names and values for this binary build
            // are added to a change set to determine which ones'
            // values differ across builds. The determination is a
            // two-part process. This is the first part. The second
            // part is happens later outside the all builds loop.
            for (name, value) in options_map.iter() {
                // Record this name (and value) if has not been seen
                // before. The count is used later to check if the
                // name is used by all, or only some, builds.
                let counter = changes.entry(name.clone()).or_insert(ChangeCounter {
                    last: value.clone(),
                    count: 0,
                    use_it: false,
                });
                counter.count += 1;

                // Is this name marked as don't use yet, and is this
                // value different from the last one seen for this
                // name?
                if !counter.use_it && counter.last != *value {
                    // The values differ, mark this name as one to use
                    counter.use_it = true;
                }
            }

            build_name_values.insert(build.ident().clone(), options_map);
        }

        // Now that all the builds have been processed, pull out the
        // option names will be used to generate build keys. This is
        // the second part of the two-part process (see above) for
        // working out what option names to use.
        let mut key_entry_names: Vec<_> = changes
            .iter()
            .filter(|(_, cc)| cc.use_it || cc.count != number_non_src_builds)
            .map(|(n, _)| n.clone())
            .collect::<Vec<_>>();

        // Sorting the names here provides a fallback alphabetical
        // order when adding to ordered_names later.
        key_entry_names.sort();

        // This sets up initial ordering of names, and thus the
        // values, within entries for the key. The ones at the front
        // are more influential in the solver than the ones at the
        // back, because their values will be earlier in the generated
        // build keys. This gives them a bigger impact on how the
        // builds are ordered when they are sorted. Only names in both
        // BUILD_KEY_NAME_ORDER and key_entry_names are added here.
        let mut ordered_names: Vec<_> = BUILD_KEY_NAME_ORDER
            .iter()
            .filter(|name| key_entry_names.contains(name))
            .cloned()
            .collect::<Vec<_>>();

        // The rest of the names not already mentioned in the
        // important BUILD_KEY_NAME_ORDER are added next. They are
        // added in alphabetical order (from above) for consistency
        // across packages and versions, but this is probably not
        // ideal for all cases. When it is detrimental, those option
        // names should be added to the configuration
        // BUILD_KEY_NAME_ORDER to ensure they fall in the correct
        // position for a site's spk setup.
        for name in key_entry_names {
            if !BUILD_KEY_NAME_ORDER.contains(&name) {
                ordered_names.push(name.clone());
            }
        }

        // Sort the builds by their generated keys generated from the
        // ordered names and values worth including.
        self.builds.make_contiguous().sort_by_cached_key(|hm| {
            // Pull an arbitrary spec out from the hashmap
            let spec = &hm.iter().next().expect("non-empty hashmap").1 .0;
            SortedBuildIterator::make_option_values_build_key(
                spec,
                &ordered_names,
                &build_name_values,
            )
        });

        // Reverse the sort to get the build with the highest
        // "numbers" in the earlier parts of its key to come first,
        // which also reverse sorts the text values, i.e. "on" will
        // come before "off".
        self.builds.make_contiguous().reverse();

        let duration: Duration = start.elapsed();
        tracing::info!(
            target: BUILD_SORT_TARGET,
            "Sort by build option values: {} builds in {} secs",
            self.builds.len(),
            duration.as_secs() as f64 + duration.subsec_nanos() as f64 * 1e-9
        );
        tracing::debug!(
            target: BUILD_SORT_TARGET,
            "Keys by build option values: built from: [{}]",
            ordered_names
                .iter()
                .map(|n| n.as_str())
                .collect::<Vec<_>>()
                .join(", "),
        );
        tracing::debug!(
            target: BUILD_SORT_TARGET,
            "Keys by build option values: 'Build => Key : Options':\n {}",
            self.builds
                .iter()
                .flat_map(|hm| hm.values())
                .map(|(spec, _)| {
                    format!(
                        "{} = {} : {:?}",
                        spec.ident(),
                        SortedBuildIterator::make_option_values_build_key(
                            spec,
                            &ordered_names,
                            &build_name_values,
                        ),
                        spec.option_values(),
                    )
                })
                .collect::<Vec<String>>()
                .join("\n ")
        );
    }
}
