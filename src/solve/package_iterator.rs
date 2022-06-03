// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use dyn_clone::DynClone;
use once_cell::sync::Lazy;
use std::ffi::OsString;
use std::time::{Duration, Instant};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{Arc, Mutex},
};

use super::solution::PackageSource;
use crate::{
    api::{self, Build, BuildKey},
    storage, Error, Result,
};

#[cfg(test)]
#[path = "./package_iterator_test.rs"]
mod package_iterator_test;

/// Allows control of which build sorting method to use in the
/// SortedBuildIterator objects: 'original' distance based, or 'new' build
/// option name value order based (the default).
// TODO: add the default value to a config file, once spk has one
static USE_ORIGINAL_BUILD_SORT: Lazy<bool> = Lazy::new(|| {
    std::env::var_os("SPK_BUILD_SORT")
        .unwrap_or_else(|| OsString::from("new"))
        .to_string_lossy()
        == "original"
});

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

// This set is for faster checks during key generation
// TODO: not sure if this matters outside python, profile it and see
static BUILD_KEY_NAME_ORDER_SET: Lazy<HashSet<String>> =
    Lazy::new(|| BUILD_KEY_NAME_ORDER.clone().into_iter().collect());

/// A list of option names to ignore during build key generation.
/// Option names in this list will not be included in build keys.
/// This list can be used to exclude names that don't help distinguish
/// a build key, e.g. 'os'. Anything not ignored will be used in the
/// keys and will need to have a sensible ordering of its values,
/// which might not be straight-forward.
/// Note: that this setting does not cause the solver to skip any
/// builds, it just won't use options with these names to help order
/// the builds.
// TODO: a the default value to config file, once spk has one
static DONT_USE_IN_KEY_NAMES: Lazy<Vec<String>> = Lazy::new(|| {
    std::env::var_os("SPK_BUILD_OPTION_KEY_DONT_USE")
        .unwrap_or_else(|| OsString::from("arch,distro,os,centos"))
        .to_string_lossy()
        .to_string()
        .split(',')
        .map(|s| s.to_string())
        .collect()
});

// This set is for faster checks during key generation
// TODO: not sure if this matters outside python, profile it and see
static DONT_USE_IN_KEY_SET: Lazy<HashSet<String>> =
    Lazy::new(|| DONT_USE_IN_KEY_NAMES.clone().into_iter().collect());

pub trait BuildIterator: DynClone + Send + Sync + std::fmt::Debug {
    fn is_empty(&self) -> bool;
    fn is_sorted_build_iterator(&self) -> bool {
        false
    }
    fn next(&mut self) -> crate::Result<Option<(Arc<api::Spec>, PackageSource)>>;
    fn version_spec(&self) -> Option<Arc<api::Spec>>;
    fn len(&self) -> usize;
}

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
    // The 'options' field is used in the by_distance based build key
    // generation. It is not used in build_options_values key
    // generation. Distance from these options can result in key
    // clashes, particularly in the early stages of a solve, that
    // didn't help distinguish builds and can create inconsistencies
    // between build picks for package versions at different levels in
    // the solve. The solver's current options are still checked
    // against builds when they are considered in the solver level,
    // which is above the build sorting level here.
    options: api::OptionMap,
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
/// across builds, or is always they same for all (non-src) builds.
/// Options with differing values across builds are worth using
/// (use_it) in a build key to distinguish builds for sorting. Options
/// that don't vary are not worth using in the build key.
struct ChangeCounter {
    pub last: String,
    pub count: u64,
    pub use_it: bool,
}

const BUILD_SORT_TARGET: &str = "build_sort";

impl SortedBuildIterator {
    pub fn new(options: api::OptionMap, source: Arc<Mutex<dyn BuildIterator>>) -> Result<Self> {
        let mut builds = VecDeque::<(Arc<api::Spec>, PackageSource)>::new();
        {
            let mut source_lock = source.lock().unwrap();
            while let Some(item) = source_lock.next()? {
                builds.push_back(item);
            }
        }

        let mut sbi = SortedBuildIterator {
            options,
            source,
            builds,
        };

        // Use the configured sort method, either the original
        // distance based one, or the build option values one.
        if *USE_ORIGINAL_BUILD_SORT {
            sbi.sort_by_distance();
        } else {
            sbi.sort_by_build_option_values();
        }
        Ok(sbi)
    }

    /// Original - Build key tuple (distance, digest) making helper
    /// function for sort_by_distance() sorting
    #[allow(clippy::nonminimal_bool)]
    fn make_distance_build_key(
        spec: &Arc<api::Spec>,
        version_spec: &Option<Arc<api::Spec>>,
        variant_count: usize,
        default_options: &api::OptionMap,
        self_options: &api::OptionMap,
    ) -> (i64, String) {
        let build = spec
            .pkg
            .build
            .as_ref()
            .map(|b| b.to_string())
            .unwrap_or_else(|| "None".to_owned());
        let total_options_count = spec.build.options.len();
        // source packages must come last to ensure that building
        // from source is the last option under normal circumstances
        if spec.pkg.build.is_none() || spec.pkg.build == Some(Build::Source) {
            return ((variant_count + total_options_count + 1) as i64, build);
        }

        if let Some(version_spec) = &version_spec {
            // if this spec is compatible with the default options, it's the
            // most valuable
            if !!&spec.build.validate_options(&spec.pkg.name, default_options) {
                return (-1, build);
            }
            // then we sort based on the first defined variant that seems valid
            for (i, variant) in version_spec.build.variants.iter().enumerate() {
                if !!&spec.build.validate_options(&spec.pkg.name, variant) {
                    return (i as i64, build);
                }
            }
        }

        // and then it's the distance from the default option set,
        // where distance is just the number of differing options
        let current_options_unfiltered = spec.resolve_all_options(&api::OptionMap::default());
        let current_options: HashSet<(&String, &String)> = current_options_unfiltered
            .iter()
            .filter(|&(o, _)| self_options.contains_key(o))
            .collect();
        let similar_options_count = default_options
            .iter()
            .collect::<HashSet<_>>()
            .intersection(&current_options)
            .collect::<HashSet<_>>()
            .len();
        let distance_from_default = (total_options_count - similar_options_count).max(0);
        ((variant_count + distance_from_default) as i64, build)
    }

    /// Original - Default options and distance based sorting
    fn sort_by_distance(&mut self) {
        let start = Instant::now();

        let version_spec = self.version_spec();
        let variant_count = version_spec
            .as_ref()
            .map(|s| s.build.variants.len())
            .unwrap_or(0);
        let default_options = version_spec
            .as_ref()
            .map(|s| s.resolve_all_options(&api::OptionMap::default()))
            .unwrap_or_else(api::OptionMap::default);

        let self_options = self.options.clone();

        self.builds
            .make_contiguous()
            .sort_by_cached_key(|(spec, _)| {
                SortedBuildIterator::make_distance_build_key(
                    spec,
                    &version_spec,
                    variant_count,
                    &default_options,
                    &self_options,
                )
            });

        // Debugging - setup in configure_logging() to appear at verbosity 3+
        let duration: Duration = start.elapsed();
        tracing::info!(
            target: BUILD_SORT_TARGET,
            "Sort by distance: {} builds in {} secs",
            self.builds.len(),
            duration.as_secs() as f64 + duration.subsec_nanos() as f64 * 1e-9
        );
        // Setup in configure_logging() to appear at verbosity 7+
        tracing::debug!(
            target: BUILD_SORT_TARGET,
            "Keys by distance: 'Build => Key : Options':\n {}",
            self.builds
                .iter()
                .map(|(spec, _)| {
                    format!(
                        "{} = {:?} : {:?}",
                        spec.pkg,
                        SortedBuildIterator::make_distance_build_key(
                            spec,
                            &version_spec,
                            variant_count,
                            &default_options,
                            &self_options,
                        ),
                        spec.resolve_all_options(&api::OptionMap::default()),
                    )
                })
                .collect::<Vec<String>>()
                .join("\n ")
        );
    }

    /// Newer - BuildKey structure making helper function for
    /// sort_by_build_option_values() sorting
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

    /// Newer - Ordered build option names and differing values based sorting
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

            // Count the number of non-src builds for later. This will
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
            // The build option names and values for this non-src
            // build are added to a change set to determine which
            // ones' values differ across builds. This determination
            // is a two-part process. This is the first part. The
            // second part is happens later outside the all builds loop.
            for (name, value) in options_map.iter() {
                if DONT_USE_IN_KEY_SET.contains(name) {
                    // Skip names configured as ones to never use
                    continue;
                }

                // Record this name and whether its value is one we've
                // seen before. The count is used later to check if
                // the name is used by all, or only some, builds.
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
            if !BUILD_KEY_NAME_ORDER_SET.contains(name) {
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

        // Debugging - setup in configure_logging() to appear at verbosity 3+
        let duration: Duration = start.elapsed();
        tracing::info!(
            target: BUILD_SORT_TARGET,
            "Sort by build option values: {} builds in {} secs",
            self.builds.len(),
            duration.as_secs() as f64 + duration.subsec_nanos() as f64 * 1e-9
        );
        // Setup in configure_logging() to appear at verbosity 7+
        tracing::debug!(
            target: BUILD_SORT_TARGET,
            "Keys by build option values: 'Build => Key : Options':\n {}",
            self.builds
                .iter()
                .map(|(spec, _)| {
                    format!(
                        "{} = {:?} : {:?}",
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
