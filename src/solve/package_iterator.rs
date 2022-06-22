// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use dyn_clone::DynClone;
use once_cell::sync::Lazy;
use std::ffi::OsString;
use std::time::{Duration, Instant};
use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
};

use super::solution::PackageSource;
use crate::{
    api::{self, BuildKey},
    storage, Error, Result,
};

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
static BUILD_KEY_NAME_ORDER: Lazy<Vec<String>> = Lazy::new(|| {
    std::env::var_os("SPK_BUILD_OPTION_KEY_ORDER")
        .unwrap_or_else(|| OsString::from("gcc,python"))
        .to_string_lossy()
        .to_string()
        .split(',')
        .map(String::from)
        .collect()
});

pub trait BuildIterator: DynClone + Send + Sync + std::fmt::Debug {
    fn is_empty(&self) -> bool;
    fn is_sorted_build_iterator(&self) -> bool {
        false
    }
    fn next(&mut self) -> crate::Result<Option<(Arc<api::Spec>, PackageSource)>>;
    fn version_spec(&self) -> Option<Arc<api::Spec>>;
    fn len(&self) -> usize;
}

/// A tracing target for additional build sorting output: times,
/// building blocks, and keys.
const BUILD_SORT_TARGET: &str = "build_sort";

dyn_clone::clone_trait_object!(BuildIterator);

type PackageIteratorItem = (api::Ident, Arc<Mutex<dyn BuildIterator>>);

pub trait PackageIterator: DynClone + Send + Sync + std::fmt::Debug {
    fn next(&mut self) -> crate::Result<Option<PackageIteratorItem>>;

    /// Replaces the internal build iterator for version with the given one.
    fn set_builds(&mut self, version: &api::Version, builds: Arc<Mutex<dyn BuildIterator>>);
}

dyn_clone::clone_trait_object!(PackageIterator);

#[derive(Clone, Debug)]
struct VersionIterator {
    versions: VecDeque<api::Version>,
}

impl Iterator for VersionIterator {
    type Item = api::Version;

    fn next(&mut self) -> Option<Self::Item> {
        self.versions.pop_front()
    }
}

impl VersionIterator {
    fn new(versions: VecDeque<api::Version>) -> Self {
        VersionIterator { versions }
    }
}

/// A stateful cursor yielding package builds from a set of repositories.
#[derive(Debug)]
pub struct RepositoryPackageIterator {
    pub package_name: api::PkgName,
    pub repos: Vec<Arc<storage::RepositoryHandle>>,
    versions: Option<VersionIterator>,
    version_map: HashMap<api::Version, Arc<storage::RepositoryHandle>>,
    builds_map: HashMap<api::Version, Arc<Mutex<dyn BuildIterator>>>,
    active_version: Option<api::Version>,
}

impl Clone for RepositoryPackageIterator {
    /// Create a copy of this iterator, with the cursor at the same point.
    fn clone(&self) -> Self {
        let version_map = if self.versions.is_none() {
            match self.build_version_map() {
                Ok(version_map) => version_map,
                Err(Error::PackageNotFoundError(_)) => {
                    return RepositoryPackageIterator::new(
                        self.package_name.clone(),
                        self.repos.clone(),
                    )
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

        RepositoryPackageIterator {
            package_name: self.package_name.clone(),
            repos: self.repos.clone(),
            versions: self.versions.clone(),
            version_map,
            // Python custom clone() doesn't clone the remaining fields
            builds_map: HashMap::default(),
            active_version: None,
        }
    }
}

impl PackageIterator for RepositoryPackageIterator {
    fn next(&mut self) -> crate::Result<Option<PackageIteratorItem>> {
        if self.versions.is_none() {
            self.start()?
        }

        if self.active_version.is_none() {
            self.active_version = self.versions.as_mut().and_then(|i| i.next());
        }
        let version = if let Some(active_version) = self.active_version.as_ref() {
            active_version
        } else {
            return Ok(None);
        };
        let repo = if let Some(repo) = self.version_map.get(version) {
            repo
        } else {
            return Err(crate::Error::String(
                "version not found in version_map".to_owned(),
            ));
        };
        let mut pkg = api::Ident::new(self.package_name.clone());
        pkg.version = version.clone();
        if !self.builds_map.contains_key(version) {
            self.builds_map.insert(
                version.clone(),
                Arc::new(Mutex::new(RepositoryBuildIterator::new(
                    pkg.clone(),
                    repo.clone(),
                )?)),
            );
        }
        let builds = self.builds_map.get(version).unwrap();
        if builds.lock().unwrap().is_empty() {
            self.active_version = None;
            return self.next();
        }
        Ok(Some((pkg, builds.clone())))
    }

    fn set_builds(&mut self, version: &api::Version, builds: Arc<Mutex<dyn BuildIterator>>) {
        self.builds_map.insert(version.clone(), builds);
    }
}

impl RepositoryPackageIterator {
    pub fn new(package_name: api::PkgName, repos: Vec<Arc<storage::RepositoryHandle>>) -> Self {
        RepositoryPackageIterator {
            package_name,
            repos,
            versions: None,
            version_map: HashMap::default(),
            builds_map: HashMap::default(),
            active_version: None,
        }
    }

    fn build_version_map(&self) -> Result<HashMap<api::Version, Arc<storage::RepositoryHandle>>> {
        let mut version_map = HashMap::default();
        for repo in self.repos.iter().rev() {
            for version in repo.list_package_versions(&self.package_name)? {
                version_map.insert(version, repo.clone());
            }
        }

        if version_map.is_empty() {
            return Err(Error::PackageNotFoundError(
                self.package_name.clone().into(),
            ));
        }

        Ok(version_map)
    }

    fn start(&mut self) -> Result<()> {
        self.version_map = self.build_version_map()?;
        let mut versions: Vec<api::Version> =
            self.version_map.keys().into_iter().cloned().collect();
        versions.sort();
        versions.reverse();
        self.versions = Some(VersionIterator::new(versions.into()));
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct RepositoryBuildIterator {
    repo: Arc<storage::RepositoryHandle>,
    builds: VecDeque<api::Ident>,
    spec: Option<Arc<api::Spec>>,
}

impl BuildIterator for RepositoryBuildIterator {
    fn is_empty(&self) -> bool {
        self.builds.is_empty()
    }

    fn next(&mut self) -> crate::Result<Option<(Arc<api::Spec>, PackageSource)>> {
        let build = if let Some(build) = self.builds.pop_front() {
            build
        } else {
            return Ok(None);
        };

        let mut spec = match self.repo.read_spec(&build) {
            Ok(spec) => spec,
            Err(Error::PackageNotFoundError(..)) => {
                tracing::warn!(
                    "Repository listed build with no spec: {} from {:?}",
                    build,
                    self.repo
                );
                return self.next();
            }
            Err(err) => return Err(err),
        };

        let components = match self.repo.get_package(&build) {
            Ok(c) => c,
            Err(Error::PackageNotFoundError(..)) => Default::default(),
            Err(err) => return Err(err),
        };

        if spec.pkg.build.is_none() {
            tracing::warn!(
                "Published spec is corrupt (has no associated build), pkg={}",
                build,
            );
            spec.pkg = spec.pkg.with_build(build.build);
        }

        Ok(Some((
            Arc::new(spec),
            PackageSource::Repository {
                repo: self.repo.clone(),
                components,
            },
        )))
    }

    fn version_spec(&self) -> Option<Arc<api::Spec>> {
        self.spec.clone()
    }

    fn len(&self) -> usize {
        self.builds.len()
    }
}

impl RepositoryBuildIterator {
    fn new(pkg: api::Ident, repo: Arc<storage::RepositoryHandle>) -> Result<Self> {
        let mut builds = repo.list_package_builds(&pkg)?;
        let spec = match repo.read_spec(&pkg) {
            Ok(spec) => Some(Arc::new(spec)),
            Err(Error::PackageNotFoundError(..)) => None,
            Err(err) => return Err(err),
        };

        // source packages must come last to ensure that building
        // from source is the last option under normal circumstances
        builds.sort_by_key(|pkg| !pkg.is_source());

        Ok(RepositoryBuildIterator {
            repo,
            builds: builds.into(),
            spec,
        })
    }
}

#[derive(Clone, Debug)]
pub struct EmptyBuildIterator {}

impl BuildIterator for EmptyBuildIterator {
    fn is_empty(&self) -> bool {
        true
    }

    fn next(&mut self) -> crate::Result<Option<(Arc<api::Spec>, PackageSource)>> {
        Ok(None)
    }

    fn version_spec(&self) -> Option<Arc<api::Spec>> {
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
    source: Arc<Mutex<dyn BuildIterator>>,
    builds: VecDeque<(Arc<api::Spec>, PackageSource)>,
}

impl BuildIterator for SortedBuildIterator {
    fn is_empty(&self) -> bool {
        self.builds.is_empty()
    }

    fn is_sorted_build_iterator(&self) -> bool {
        true
    }

    fn next(&mut self) -> crate::Result<Option<(Arc<api::Spec>, PackageSource)>> {
        Ok(self.builds.pop_front())
    }

    fn version_spec(&self) -> Option<Arc<api::Spec>> {
        self.source.lock().unwrap().version_spec()
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
    pub fn new(_options: api::OptionMap, source: Arc<Mutex<dyn BuildIterator>>) -> Result<Self> {
        // Note: _options is unused in this implementation, it was used
        // in the by_distance sorting implementation
        let mut builds = VecDeque::<(Arc<api::Spec>, PackageSource)>::new();
        {
            let mut source_lock = source.lock().unwrap();
            while let Some(item) = source_lock.next()? {
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
        spec: &api::Spec,
        ordered_names: &Vec<String>,
        build_name_values: &HashMap<String, api::OptionMap>,
    ) -> BuildKey {
        let build_id = spec.pkg.to_string();
        let empty = api::OptionMap::default();
        let name_values = match build_name_values.get(&build_id) {
            Some(nv) => nv,
            None => &empty,
        };
        BuildKey::new(&spec.pkg, ordered_names, name_values)
    }

    /// Sorts builds by keys based on ordered build option names and
    /// differing values in those options
    fn sort_by_build_option_values(&mut self) {
        let start = Instant::now();

        let mut number_non_src_builds: u64 = 0;
        let mut build_name_values: HashMap<String, api::OptionMap> = HashMap::default();
        let mut changes: HashMap<String, ChangeCounter> = HashMap::new();

        // Get all the build option names across all the builds, and
        // their values.
        for (build, _) in &self.builds {
            // Skip this if it's '/src' build because '/src' builds
            // won't use the build option values in their key, they
            // don't need to be looked at. They have a type of key
            // that always puts them last in the build order.
            if let Some(b) = &build.pkg.build {
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
            let options_map = build.resolve_all_options(&api::OptionMap::default());

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

            build_name_values.insert(build.pkg.to_string(), options_map);
        }

        // Now that all the builds have been processed, pull out the
        // option names will be used to generate build keys. This is
        // the second part of the two-part process (see above) for
        // working out what option names to use.
        let mut key_entry_names: Vec<String> = changes
            .iter()
            .filter(|(_, cc)| cc.use_it || cc.count != number_non_src_builds)
            .map(|(n, _)| n.clone())
            .collect::<Vec<String>>();

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
        let mut ordered_names: Vec<String> = BUILD_KEY_NAME_ORDER
            .iter()
            .filter(|name| key_entry_names.contains(name))
            .map(ToString::to_string)
            .collect::<Vec<String>>();

        // The rest of the names not already mentioned in the
        // important BUILD_KEY_NAME_ORDER are added next. They are
        // added in alphabetical order (from above) for consistency
        // across packages and versions, but this is probably not
        // ideal for all cases. When it is detrimental, those option
        // names should be added to the configuration
        // BUILD_KEY_NAME_ORDER to ensure they fall in the correct
        // position for a site's spk setup.
        for name in &key_entry_names {
            if !BUILD_KEY_NAME_ORDER.contains(name) {
                ordered_names.push(name.clone());
            }
        }

        // Sort the builds by their generated keys generated from the
        // ordered names and values worth including.
        self.builds
            .make_contiguous()
            .sort_by_cached_key(|(spec, _)| {
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
            "Keys by build option values: buildt from: [{}]",
            ordered_names.join(", "),
        );
        tracing::debug!(
            target: BUILD_SORT_TARGET,
            "Keys by build option values: 'Build => Key : Options':\n {}",
            self.builds
                .iter()
                .map(|(spec, _)| {
                    format!(
                        "{} = {} : {:?}",
                        spec.pkg,
                        SortedBuildIterator::make_option_values_build_key(
                            spec,
                            &ordered_names,
                            &build_name_values,
                        ),
                        spec.resolve_all_options(&api::OptionMap::default()),
                    )
                })
                .collect::<Vec<String>>()
                .join("\n ")
        );
    }
}
