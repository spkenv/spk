// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::collections::BTreeSet;
use std::iter::zip;
use std::sync::Arc;

use async_stream::try_stream;
use futures::{Stream, TryStreamExt};
use itertools::Itertools;
use miette::{Result, miette};
use nom::combinator::all_consuming;
use spfs::Digest;
use spfs::graph::DatabaseView;
use spfs::graph::object::Enum;
use spk_schema::foundation::ident_component::Component;
use spk_schema::foundation::name::PkgNameBuf;
use spk_schema::ident::{AnyIdent, AsVersionIdent};
use spk_schema::ident_ops::parsing::ident_parts_with_components;
use spk_schema::option_map::OptFilter;
use spk_schema::spec_ops::WithVersion;
use spk_schema::{BuildIdent, Deprecate, Package, Spec, VersionIdent};

use crate::{RepositoryHandle, storage};

#[cfg(test)]
#[path = "./walker_test.rs"]
mod walker_test;

/// Deprecation states for walked versions (based on their deprecated builds)
#[derive(Debug, Default, PartialEq, Eq, Clone)]
pub enum DeprecationState {
    #[default]
    NotCalculated,
    Active,
    PartiallyDeprecated,
    Deprecated,
}

/// A repository the RepoWalker is processing
#[derive(Debug, PartialEq)]
pub struct WalkedRepo<'a> {
    pub name: &'a str,
}

/// A package in a repo the RepoWalker is processing
#[derive(Clone, Debug, PartialEq)]
pub struct WalkedPackage<'a> {
    pub repo_name: &'a str,
    pub name: Arc<PkgNameBuf>,
}

/// A version of a package the RepoWalker is processing
#[derive(Clone, Debug, PartialEq)]
pub struct WalkedVersion<'a> {
    pub repo_name: &'a str,
    /// This doesn't include the package name as a separate field
    /// because it is included in the version ident.
    pub ident: Arc<VersionIdent>,
    pub deprecation_state: DeprecationState,
}

/// A build of a version the RepoWalker is processing
#[derive(Clone, Debug, PartialEq)]
pub struct WalkedBuild<'a> {
    pub repo_name: &'a str,
    /// This doesn't include the build ident, because the spec.ident()
    /// method can provide it.
    pub spec: Arc<Spec>,
}

/// A component of a build the RepoWalker is processing
#[derive(Clone, Debug, PartialEq)]
pub struct WalkedComponent<'a> {
    pub repo_name: &'a str,
    pub build: Arc<BuildIdent>,
    pub name: Component,
    pub digest: Arc<Digest>,
}

/// A file in a component the RepoWalker is processing
#[derive(Debug, PartialEq)]
pub struct WalkedFile<'a> {
    pub repo_name: &'a str,
    pub build: Arc<BuildIdent>,
    pub component: Component,
    pub path_pieces: Vec<Arc<String>>,
    pub entry: spfs::tracking::Entry,
}

/// The items a RepoWalker can find and return during a walk
#[derive(Debug, PartialEq)]
pub enum RepoWalkerItem<'a> {
    // Ones emitted during standard walks
    Repo(WalkedRepo<'a>),
    Package(WalkedPackage<'a>),
    Version(WalkedVersion<'a>),
    Build(WalkedBuild<'a>),
    Component(WalkedComponent<'a>),
    File(WalkedFile<'a>),
    // Ones only emitted when the "EndOf..." markers are enabled
    EndOfComponent(WalkedComponent<'a>),
    EndOfBuild(WalkedBuild<'a>),
    EndOfVersion(WalkedVersion<'a>),
    EndOfPackage(WalkedPackage<'a>),
    EndOfRepo(WalkedRepo<'a>),
}

/// Package filters will be given the repository name, and the package name
pub type PackageFilterFunc<'a> = dyn Fn(&WalkedPackage) -> bool + Send + Sync + 'a;
/// Version filters will be  given the version object
pub type VersionFilterFunc<'a> = dyn Fn(&WalkedVersion) -> bool + Send + Sync + 'a;
// TODO: We don't have a use case for this at the moment do we? It could be added for completeness
// pub type VersionRecipeFilterFunc<'a> = dyn Fn(&Arc<Recipe>) -> bool + Send + Sync + 'a;
/// Build ident filters will be given the build's Ident
pub type BuildIdentFilterFunc<'a> = dyn Fn(&BuildIdent) -> bool + Send + Sync + 'a;
/// Build spec filters will be given the build's spec
pub type BuildSpecFilterFunc<'a> = dyn Fn(&WalkedBuild) -> bool + Send + Sync + 'a;
/// Component filters will be given the component object
pub type ComponentFilterFunc<'a> = dyn Fn(&WalkedComponent) -> bool + Send + Sync + 'a;
/// File filters will be given the spfs entry object, and a list of the parent path fragments to the entry
pub type FileFilterFunc<'a> = dyn Fn(&WalkedFile) -> bool + Send + Sync + 'a;

/// A place to keep the default and commonly used filter functions
/// that the RepoWalkerBuilder and callers may use to configure a
/// RepoWalker's filter functions. This contains standard filters used
/// by the ls, search, stats, du commands for their walkers.
pub struct RepoWalkerFilter;

impl RepoWalkerFilter {
    /// Match any package and repo name
    pub fn no_package_filter(_package: &WalkedPackage) -> bool {
        true
    }

    /// Match any version and package name
    pub fn no_version_filter(_version: &WalkedVersion) -> bool {
        true
    }

    /// Match any build ident
    pub fn no_build_ident_filter(_build_ident: &BuildIdent) -> bool {
        true
    }

    /// Match match any build spec
    pub fn no_build_spec_filter(_build: &WalkedBuild) -> bool {
        true
    }

    /// Match any component and build ident
    pub fn no_component_filter(_component: &WalkedComponent) -> bool {
        true
    }

    /// Match any (file) entry, under any parent path, from any build,
    /// component or digest
    pub fn no_file_filter(_file: &WalkedFile) -> bool {
        true
    }

    /// Check the given package name is the same as the package name
    /// being looked for, and that the repository name, if specified,
    /// matches as well.
    pub fn exact_package_name_filter(
        package: &WalkedPackage,
        repository_name_to_match: Option<String>,
        pkg_name_to_match: String,
    ) -> bool {
        if let Some(rn) = repository_name_to_match {
            if *package.repo_name != rn {
                // A repo name given and it didn't match, so don't
                // need to check any further.
                return false;
            }
        }
        ***package.name == pkg_name_to_match
    }

    /// Returns true if the given substring is in the package name
    pub fn substring_package_name_filter(package: &WalkedPackage, substring: String) -> bool {
        package.name.contains(&substring)
    }

    /// Returns true if the version, as a string, matches the given version string
    pub fn exact_match_version_filter(version: &WalkedVersion, version_to_match: String) -> bool {
        *version.ident.version() == version_to_match
    }

    /// Returns true if the build ident, as a string, matches the given build string
    pub fn exact_match_build_digest_filter(build: &BuildIdent, search_build: String) -> bool {
        build.build().to_string() == search_build
    }

    /// Returns true if the build spec contains matches for all the
    /// given build options OptFilters
    pub fn match_build_options_filter(build: &WalkedBuild, build_options: Vec<OptFilter>) -> bool {
        build.spec.matches_all_filters(&Some(build_options))
    }

    /// Returns true if the component is in the set of allowed components
    pub fn allowed_components_filter(
        component: &WalkedComponent,
        allowed_components: &BTreeSet<Component>,
    ) -> bool {
        if allowed_components.is_empty() {
            true
        } else {
            allowed_components.contains(&component.name)
        }
    }

    /// Returns true if the parent filepath pieces match up with the
    /// given list of filepath pieces, as far as they are specified.
    pub fn parent_paths_match(file: &WalkedFile, filepaths: &Vec<String>) -> bool {
        for (path_fragment, search_fragment) in zip(file.path_pieces.clone(), filepaths) {
            if **path_fragment != **search_fragment {
                return false;
            }
        }
        true
    }
}

/// A single stream objects found by walking a list of spk repos. It
/// returns the objects from top to bottom and depth-first: the repos,
/// packages, versions, builds, components, and then files.
///
/// A RepoWalker supports various search options but cannot be
/// constructed directly. It is configured and constructed via a
/// RepoWalkerBuilder. Calling walk() on a RepoWalker produces a
/// stream of RepoWalkerItem objects for the things it finds.
///
/// A RepoWalker can be configured to emit an 'EndOf...' item when it
/// finishes walking all the sub-objects of an object in the
/// hierarchy, e.g. an 'EndOfVersion' it emitted after the last Build
/// in that Version has been processed.
pub struct RepoWalker<'a> {
    repos: &'a Vec<(String, storage::RepositoryHandle)>,
    /// There is no repository filter function because the list of
    /// repos to walk is given to the walker when it is created from
    /// the builder. If the caller doesn't want a repo walked, they
    /// should not pass it to the builder.
    package_filter_func: Arc<PackageFilterFunc<'a>>,
    version_filter_func: Arc<VersionFilterFunc<'a>>,
    // TODO: We don't have a use case for this at the moment, do we?
    // version_recipe_filter_func: Arc<VersionRecipeFilterFunc<'a>>,
    build_ident_filter_func: Arc<BuildIdentFilterFunc<'a>>,
    build_spec_filter_func: Arc<BuildSpecFilterFunc<'a>>,
    component_filter_func: Arc<ComponentFilterFunc<'a>>,
    file_filter_func: Arc<FileFilterFunc<'a>>,

    /// Object level reporting controls, used to limit the extent of
    /// the walk by the kinds of objects.
    report_on_versions: bool,
    report_on_builds: bool,
    /// Build specific settings
    report_src: bool,
    report_deprecated: bool,
    report_embedded: bool,
    /// The objects inside a build
    report_on_components: bool,
    report_on_files: bool,
    /// Whether to emit the EndOf... items from the stream or not
    emit_end_of_markers: bool,
    /// Whether to turn errors into warning and continue the walking
    /// from the next object instead of stopping when an error occurs.
    continue_on_error: bool,
    /// Whether to sort objects at each level before walking them.
    sort_objects: bool,
    /// Whether to only walk the highest version and then move on to the next package
    highest_version_only: bool,
    /// Whether to work out if a version is deprecated before emitting it
    calculate_deprecated_versions: bool,
}

impl std::fmt::Debug for RepoWalker<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The *_filter_func fields are not output
        f.debug_struct("RepoWalker")
            .field("report on versions", &self.report_on_versions)
            .field("report on builds", &self.report_on_builds)
            .field("include /src builds", &self.report_src)
            .field("include deprecated builds", &self.report_deprecated)
            .field("include embedded builds", &self.report_embedded)
            .field("report on components", &self.report_on_components)
            .field("report on files", &self.report_on_files)
            .field("emit EndOf markers", &self.emit_end_of_markers)
            .field("continue on error", &self.continue_on_error)
            .field("sort objects", &self.sort_objects)
            .field(
                "repos",
                &self.repos.iter().map(|(n, _)| n.to_string()).join(","),
            )
            .finish()
    }
}

impl RepoWalker<'_> {
    // Get all the filtered packages from the repo
    async fn get_matching_packages<'a>(
        &self,
        repository_name: &'a str,
        repo: &RepositoryHandle,
    ) -> Result<Vec<WalkedPackage<'a>>> {
        let mut packages = match repo.list_packages().await {
            Ok(pkgs) => pkgs
                .into_iter()
                .filter_map(|p| {
                    // Filter on matching packages, which can check
                    // both the package name and the repo name. This
                    // comes from the spk ls usage.
                    let package = WalkedPackage {
                        repo_name: repository_name,
                        name: Arc::new(p),
                    };

                    if (self.package_filter_func)(&package) {
                        Some(package)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>(),
            Err(err) => {
                if self.continue_on_error {
                    tracing::warn!("{err}");
                    Vec::new()
                } else {
                    return Err(err.into());
                }
            }
        };

        if self.sort_objects {
            packages.sort_by_cached_key(|p| p.name.clone());
        }
        Ok(packages)
    }

    // Get the filtered versions, if enabled
    async fn get_matching_versions<'a>(
        &self,
        repository_name: &'a str,
        repo: &RepositoryHandle,
        package_name: &PkgNameBuf,
    ) -> Result<Vec<WalkedVersion<'a>>> {
        // If this walker is configured to stop at packages, don't
        // return any versions.
        if !self.report_on_versions {
            return Ok(Vec::new());
        }

        let base = AnyIdent::from(package_name.clone());
        let mut versions = match repo.list_package_versions(base.name()).await {
            Ok(vers) => vers
                .iter()
                .filter_map(|v| {
                    let v_id: VersionIdent =
                        base.with_version((**v).clone()).as_version_ident().clone();
                    let version = WalkedVersion {
                        repo_name: repository_name,
                        ident: Arc::new(v_id),
                        deprecation_state: DeprecationState::NotCalculated,
                    };

                    if (self.version_filter_func)(&version) {
                        Some(version)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>(),
            Err(err) => {
                if self.continue_on_error {
                    tracing::warn!("{err}");
                    Vec::new()
                } else {
                    return Err(err.into());
                }
            }
        };

        // TODO: any matching on a version's recipe could go here if needed.

        if self.sort_objects {
            versions.sort_by_cached_key(|v| std::cmp::Reverse(v.ident.clone()));
        }

        if self.highest_version_only {
            // Replace the versions with the highest numbered one only
            versions = versions.into_iter().take(1).collect();
        }

        Ok(versions)
    }

    // Get the filtered builds, if enabled
    async fn get_matching_builds<'a>(
        &self,
        repository_name: &'a str,
        repo: &RepositoryHandle,
        version_ident: &VersionIdent,
    ) -> Result<(Vec<WalkedBuild<'a>>, DeprecationState)> {
        // If this walker is configured to stop at versions, don't
        // return any builds.
        if !self.report_on_builds {
            return Ok((Vec::new(), DeprecationState::NotCalculated));
        }

        let mut build_idents = match repo.list_package_builds(version_ident).await {
            Ok(bs) => bs,
            Err(err) => {
                if self.continue_on_error {
                    tracing::warn!("{err}");
                    Vec::new()
                } else {
                    return Err(err.into());
                }
            }
        };

        if self.sort_objects {
            build_idents.sort();
        }

        let version_deprecation = self
            .calculate_version_deprecation_state(repo, &build_idents)
            .await;

        // Only keep the matching builds
        let mut results = Vec::new();
        for build_id in build_idents.into_iter() {
            if let Some(b) = self.build_matches(repository_name, repo, &build_id).await? {
                results.push(b)
            }
        }

        Ok((results, version_deprecation))
    }

    async fn calculate_version_deprecation_state(
        &self,
        repo: &RepositoryHandle,
        build_idents: &Vec<BuildIdent>,
    ) -> DeprecationState {
        if self.calculate_deprecated_versions {
            // Check deprecation status of each build to work out the
            // version's deprecation state.
            let mut any_deprecated = false;
            let mut any_not_deprecated = false;

            for build_id in build_idents {
                if let Ok(spec) = repo.read_package(build_id).await {
                    if spec.is_deprecated() {
                        any_deprecated = true;
                    } else {
                        any_not_deprecated = true;
                    }
                }
                if any_deprecated && any_not_deprecated {
                    // Have checked enough builds to work out the
                    // version's deprecation state
                    break;
                }
            }

            let all_deprecated = any_deprecated && !any_not_deprecated;

            if all_deprecated {
                DeprecationState::Deprecated
            } else if any_deprecated {
                DeprecationState::PartiallyDeprecated
            } else {
                DeprecationState::Active
            }
        } else {
            DeprecationState::NotCalculated
        }
    }

    // Check the build against the walker's global builds settings and
    // the two configurable build filtering functions: one on idents,
    // and one on specs. If the build ident and spec pass all the
    // checks, this returns the build's spec.
    async fn build_matches<'a>(
        &self,
        repository_name: &'a str,
        repo: &RepositoryHandle,
        build: &BuildIdent,
    ) -> Result<Option<WalkedBuild<'a>>> {
        // Check the build ident against the ident level filter
        if !(self.build_ident_filter_func)(build) {
            return Ok(None);
        }

        // Run the faster ident based checks, if configured
        if !self.report_src && build.is_source() {
            return Ok(None);
        }

        if !self.report_embedded && build.is_embedded() {
            return Ok(None);
        }

        // At this point, the faster ident based checks have passed.
        // So read in the build spec for the slower, spec based checks.
        let spec = match repo.read_package(build).await {
            Ok(s) => s,
            Err(err) => {
                if self.continue_on_error {
                    tracing::warn!("{err}");
                    return Ok(None);
                } else {
                    return Err(err.into());
                }
            }
        };

        // Check the build spec against the spec level filters, if configured
        if !self.report_deprecated && spec.is_deprecated() {
            // Filter out deprecated builds
            return Ok(None);
        }

        let b = WalkedBuild {
            repo_name: repository_name,
            spec: Arc::clone(&spec),
        };

        if !(self.build_spec_filter_func)(&b) {
            // Filter out builds that don't match the option/host os filters
            return Ok(None);
        }

        Ok(Some(b))
    }

    // Get the filtered components, if enabled
    async fn get_matching_components<'a>(
        &self,
        repository_name: &'a str,
        repo: &RepositoryHandle,
        build_ident: Arc<BuildIdent>,
    ) -> Result<Vec<WalkedComponent<'a>>> {
        // If this walker is configured to stop at builds, don't
        // return any components.
        if !self.report_on_components {
            return Ok(Vec::new());
        }

        let components = match repo.read_components(&build_ident).await {
            Ok(cs) => cs.into_iter().filter_map(|(name, digest)| {
                let component = WalkedComponent {
                    repo_name: repository_name,
                    build: Arc::clone(&build_ident),
                    name,
                    digest: Arc::new(digest),
                };

                if (self.component_filter_func)(&component) {
                    Some(component)
                } else {
                    None
                }
            }),
            Err(err) => {
                if self.continue_on_error {
                    tracing::warn!("{err}");
                    return Ok(Vec::new());
                } else {
                    return Err(err.into());
                }
            }
        };

        if self.sort_objects {
            // Walked components are sorted only by their component name
            Ok(components
                .into_iter()
                .sorted_by_cached_key(|c| c.name.clone())
                .collect())
        } else {
            Ok(components.into_iter().collect())
        }
    }

    // Helper to wrap the file stream output to check for errors, and
    // if configured convert then into warnings.
    fn convert_file_error<'a>(
        &self,
        file_item: Result<Option<WalkedFile<'a>>>,
    ) -> Result<Option<WalkedFile<'a>>> {
        match file_item {
            Ok(f) => Ok(f),
            Err(err) => {
                if self.continue_on_error {
                    tracing::warn!("{err}");
                    Ok(None)
                } else {
                    Err(err)
                }
            }
        }
    }

    // TODO: This could be a spfs level storage walker, or use one in
    // future. This processes spfs objects, but only returns file things
    // that spk wants to know about, i.e. WalkedFiles.
    pub fn file_stream<'a>(
        &'a self,
        repo: &'a RepositoryHandle,
        component: WalkedComponent<'a>,
    ) -> impl Stream<Item = Result<WalkedFile<'a>>> + 'a {
        Box::pin(try_stream! {
            // If this walker is configured to stop at component, don't
            // return any files.
            if !self.report_on_files {
                return;
            }

            // This only stream only operates on spfs repos
            let storage::RepositoryHandle::SPFS(spfs_repo) = repo else {
                return;
            };

            // Prime the processing list with the item behind the given digest
            let mut item = spfs_repo.read_object(*component.digest).await?;
            let mut items_to_process: Vec<spfs::graph::Object> = vec![item];

            while !items_to_process.is_empty() {
                let mut next_iter_objects: Vec<spfs::graph::Object> = Vec::new();

                for object in items_to_process.iter() {
                    match object.to_enum() {
                        Enum::Platform(object) => {
                            for digest in object.iter_bottom_up() {
                                item = spfs_repo.read_object(*digest).await?;
                                next_iter_objects.push(item);
                            }
                        }
                        Enum::Layer(object) => {
                            let manifest_digest = match object.manifest() {
                                None => continue,
                                Some(d) => d,
                            };
                            item = spfs_repo.read_object(*manifest_digest).await?;
                            next_iter_objects.push(item);
                        }
                        Enum::Manifest(object) => {
                            // Manifests contain the files, and the directories paths to the files
                            let tracking_manifest = object.to_tracking_manifest();
                            let root_entry = tracking_manifest.take_root();

                            // The stack will contain pairs of a list (stack) of child entries and
                            // parent's path fragments.
                            let mut stack = Vec::new();
                            stack.push(vec![(root_entry, Vec::new())]);

                            // Depth first traversal - pre-order - using a stack of stacks
                            while !stack.is_empty() {
                                if let Some(mut next_children) = stack.pop() {
                                    // Get the next entry to process from the first stack of child entries
                                    if let Some((next_child_entry, parent_paths)) = next_children.pop() {

                                        // Put the rest of these child entries back on the main processing stack
                                        if !next_children.is_empty() {
                                            stack.push(next_children);
                                        }

                                        let f = WalkedFile {
                                            repo_name: repo.name(),
                                            build: Arc::clone(&component.build),
                                            component: component.name.clone(),
                                            path_pieces: parent_paths.clone(),
                                            entry: next_child_entry.clone(),
                                        };
                                        if !(self.file_filter_func)(&f) {
                                            // Filter out entries (dirs/files) that don't match
                                            continue;
                                        }

                                        if next_child_entry.kind.is_blob() {
                                            // A Blob entry in a Manifest represents a file - emit it
                                            yield f;
                                        }

                                        // Make a stack of this entry's children, pair it and the parent paths,
                                        // and put the pair onto the processing stack
                                        if !next_child_entry.entries.is_empty() {
                                            let mut children = Vec::with_capacity(next_child_entry.entries.len());
                                            for (path, child_entry) in
                                                next_child_entry.entries.iter().sorted_by_key(|(k, _)| *k).rev()
                                            {
                                                let mut updated_paths = parent_paths.clone();
                                                updated_paths.push(Arc::from(path.as_str().to_string()));
                                                children.push((child_entry.clone(), updated_paths));
                                            }
                                            stack.push(children);
                                        }
                                    }
                                }
                            }
                        }
                        Enum::Blob(_object) => {
                            // These are ignored for finding files
                            continue;
                        }
                    }
                }

                // Update the processing list with the objects found in platforms and layers
                items_to_process = std::mem::take(&mut next_iter_objects);
            }
        })
    }

    /// Walk the spk objects in the repos and stream back the matching
    /// ones based on the walker's configuration.
    pub fn walk(&self) -> impl Stream<Item = Result<RepoWalkerItem>> + '_ {
        Box::pin(try_stream! {
            for (repository_name, repo) in self.repos.iter() {
                let repo_name = repository_name.as_str();
                yield RepoWalkerItem::Repo(WalkedRepo { name: repo_name } );

                let packages = self.get_matching_packages(repository_name, repo).await?;
                for package in packages.iter() {
                    yield RepoWalkerItem::Package(package.clone());

                    let package_name = Arc::clone(&package.name);

                    let versions = self.get_matching_versions(repository_name, repo, &package_name).await?;
                    for package_version in versions.iter() {
                        let ident = Arc::clone(&package_version.ident);

                        let (builds, version_deprecation_state) = self.get_matching_builds(repository_name, repo, &ident).await?;
                        if self.calculate_deprecated_versions {
                            let mut version = package_version.clone();
                            version.deprecation_state = version_deprecation_state.clone();
                            yield RepoWalkerItem::Version(version);
                        } else {
                            yield RepoWalkerItem::Version(package_version.clone());
                        }

                        for build in builds.iter() {
                            yield RepoWalkerItem::Build(build.clone());

                            let build_ident = Arc::new(build.spec.ident().clone());
                            let components = self.get_matching_components(repository_name, repo, build_ident).await?;
                            for component in components.iter() {
                                yield RepoWalkerItem::Component(component.clone());

                                let mut at_least_one_file = false;
                                let mut file_stream = self.file_stream(repo, component.clone());
                                while let Some(file_item) = self.convert_file_error(file_stream.try_next().await)? {
                                    yield RepoWalkerItem::File(file_item);
                                    at_least_one_file = true;
                                }

                                if self.emit_end_of_markers && at_least_one_file {
                                    // Report the end of this component (after all its files and if it had any files)
                                    yield RepoWalkerItem::EndOfComponent(component.clone())
                                }
                            }

                            if self.emit_end_of_markers && !components.is_empty() {
                                // Report the end of this build (after all the components in it)
                                yield RepoWalkerItem::EndOfBuild(build.clone())
                            }
                        }

                        if self.emit_end_of_markers && !builds.is_empty() {
                            // Report the end of this version (after all its builds)
                            if self.calculate_deprecated_versions {
                                let mut version = package_version.clone();
                                version.deprecation_state = version_deprecation_state;
                                yield RepoWalkerItem::EndOfVersion(version)
                            } else {
                                yield RepoWalkerItem::EndOfVersion(package_version.clone())
                            }
                        }
                    }

                    if self.emit_end_of_markers && !versions.is_empty() {
                        // Report the end of this package (after all its versions)
                        yield RepoWalkerItem::EndOfPackage(package.clone())
                    }
                }

                if self.emit_end_of_markers && !packages.is_empty() {
                    // Report the end of this repo (after all the packages in it)
                    yield RepoWalkerItem::EndOfRepo(WalkedRepo { name: repo_name })
                }
            }
        })
    }
}

/// A builder for constructing a RepoWalker from various settings.
///
/// A default RepoWalker can made with:
/// ```
/// use spk_storage::RepoWalkerBuilder;
/// # use spk_storage::local_repository;
/// # use spk_storage::Result;
/// # use futures::executor::block_on;
/// # fn main() -> Result<()> {
/// # let mut repo = block_on(local_repository())?;
/// # let repos = vec!(("local".to_string(), repo.into()));
///
/// let repo_walker_builder = RepoWalkerBuilder::new(&repos);
/// let repo_walker = repo_walker_builder.build();
/// # Ok(())
/// # }
/// ```
/// That makes a RepoWalker that will: walk the given list of
/// repositories to report on all packages, all versions, all builds
/// that aren't /src or /deprecated builds, and will emit the package,
/// version, and builds objects in sorted order (packages by name,
/// versions by highest first, builds by digest). It will stop at
/// builds, and will not walk components or files, and it won't emit
/// "EndOf..." object markers. It will stop if it hits an error.
///
/// If the caller doesn't want a repository walked, they should not
/// include it in the list given to the RepoWalkerBuilder's
/// constructor.
///
/// Other walkers can be made by using the with_* methods on the
/// RepoWalkerBuilder to configure it before calling build() and
/// making the RepoWalker, e.g.
/// ```
/// use spk_storage::RepoWalkerBuilder;
/// # use spk_storage::local_repository;
/// # use futures::executor::block_on;
/// # fn main() -> miette::Result<()> {
/// # let mut repo = block_on(local_repository())?;
/// # let repos = vec!(("local".to_string(), repo.into()));
/// let some_package = Some("python/3.10.10".to_string());
/// let host_options = None;
/// let some_file_path = Some("/lib".to_string());
///
/// let mut repo_walker_builder = RepoWalkerBuilder::new(&repos);
/// let repo_walker = repo_walker_builder
///       .try_with_package_equals(&some_package)?
///       .with_report_on_versions(true)
///       .with_report_on_builds(true)
///       .with_report_src_builds(true)
///       .with_report_deprecated_builds(true)
///       .with_report_embedded_builds(false)
///       .with_report_on_components(true)
///       .with_build_options_matching(host_options)
///       .with_report_on_files(true)
///       .with_file_path(some_file_path)
///       .with_continue_on_error(true)
///       .build();
/// # Ok(())
/// # }
/// ```
/// That makes a RepoWalker that will: walk down to files, but only
/// for packages that match the given package identifier. It will
/// filter out embedded builds and builds that don't match given host
/// options. It will turn errors into warnings and continue on if it
/// hits an error.
#[derive(Clone)]
pub struct RepoWalkerBuilder<'a> {
    /// A list of repositories to walk. These must be given to the
    /// constructor, everything else has a default, see new(). There
    /// is no repository filter function because of this.
    repos: &'a Vec<(String, storage::RepositoryHandle)>,
    /// Filter function settings
    package_filter_func: Arc<PackageFilterFunc<'a>>,
    version_filter_func: Arc<VersionFilterFunc<'a>>,
    // TODO: We don't have a use case for this at the moment do we?
    // version_recipe_filter_func: Arc<VersionRecipeFilterFunc<'a>>,
    build_ident_filter_func: Arc<BuildIdentFilterFunc<'a>>,
    build_spec_filter_func: Arc<BuildSpecFilterFunc<'a>>,
    component_filter_func: Arc<ComponentFilterFunc<'a>>,
    file_filter_func: Arc<FileFilterFunc<'a>>,
    /// Reporting  controls
    report_on_versions: bool,
    report_on_builds: bool,
    /// Build specific controls
    report_src: bool,
    report_deprecated: bool,
    report_embedded: bool,
    /// Below sub-build object reporting controls
    report_on_components: bool,
    report_on_files: bool,
    /// Whether to emit end of object level markers, e.g. EndOfVersion
    /// once all the builds have been walked.
    end_of_markers: bool,
    /// Whether to turn errors into warning log messages and continue walking
    continue_on_error: bool,
    /// Whether to sort the objects returned from walker
    sort_objects: bool,
    /// Whether to only emit the highest version, and things beneath it, for each package
    highest_version_only: bool,
    /// Whether to work out if a version is deprecated before emitting it
    /// This requires processing the version's builds before emitting the version.
    calculate_deprecated_versions: bool,
}

impl<'a> RepoWalkerBuilder<'a> {
    /// Create a new RepoWalkerBuilder with the given repositories and
    /// these defaults:
    /// - no filter functions, so will emit everything it finds except where stated below,
    /// - it will report on: packages, versions, normal and embedded builds
    /// - it will not report on /src or deprecated builds
    /// - it will not report on, or walk, components and files
    /// - it will not emit EndOf markers
    /// - it will stop if it encounters an error
    /// - it will sort the objects i it find before emitting them
    ///
    /// Effectively this will walk all the active builds in the repos.
    pub fn new(repos: &'a Vec<(String, storage::RepositoryHandle)>) -> Self {
        Self {
            repos,
            // Allows everything from all the given repositories by default
            package_filter_func: Arc::new(RepoWalkerFilter::no_package_filter),
            version_filter_func: Arc::new(RepoWalkerFilter::no_version_filter),
            build_ident_filter_func: Arc::new(RepoWalkerFilter::no_build_ident_filter),
            build_spec_filter_func: Arc::new(RepoWalkerFilter::no_build_spec_filter),
            component_filter_func: Arc::new(RepoWalkerFilter::no_component_filter),
            file_filter_func: Arc::new(RepoWalkerFilter::no_file_filter),
            // Show everything down to, and including, the builds by default
            report_on_versions: true,
            report_on_builds: true,
            // Don't show source or deprecated builds by default
            report_src: false,
            report_deprecated: false,
            // Show embedded builds by default
            report_embedded: true,
            // Don't consider, or report on, things below builds by default
            report_on_components: false,
            report_on_files: false,
            // Additional output controls
            // Do not emit EndOf... markers by default
            end_of_markers: false,
            // Error normally by default
            continue_on_error: false,
            // Sort objects before emitting them by default
            sort_objects: true,
            // Include all the versions by default
            highest_version_only: false,
            // Do not calculated deprecated versions by default
            calculate_deprecated_versions: false,
        }
    }

    /// Given a string, use it to set up substring matching for
    /// package names. This is a helper function used by the spk
    /// search command. The same filter could be set up directly using
    /// [Self::with_package_filter].
    pub fn with_package_name_substring_matching(&mut self, search_substring: String) -> &mut Self {
        self.with_package_filter(move |wp| {
            RepoWalkerFilter::substring_package_name_filter(wp, search_substring.clone())
        });
        self
    }

    /// If given some package identifying string, which could include
    /// a package name, a version components, and a build, parse it and
    /// use it to configure filtering to match each identifying part
    /// of the string. This will fail and return a parsing error, if
    /// the string does not parse as a package identifier of the form:
    /// package:{components}/version/build
    ///
    /// If given None instead of a package string, this will not change
    /// any filtering functions. It will not reset the filters to the
    /// defaults.
    ///
    /// If given some identifier, at a minimum this will setup up a
    /// filter to match the package name exactly. At most, it can
    /// setup filters to match the exact repo and package name, exact
    /// version number, exact build digest, and exact components.
    ///
    /// For example: given "some_package/1.2.3" this will set up a filter
    /// that matches the "some_package" name exactly, and a filter that
    /// matches the "1.2.3" number exactly.
    ///
    /// This is a helper function used by the spk ls, du, and stats
    /// commands. The same kinds of filters could also be set up by
    /// calling: [Self::with_package_filter], [Self::with_version_filter],
    /// [Self::with_build_ident_filter], and [Self::with_component_filter]
    pub fn try_with_package_equals(
        &mut self,
        search_package: &'a Option<String>,
    ) -> Result<&mut Self> {
        match search_package {
            Some(package) => {
                // Parse the string into package identifying parts
                let (_, (parts, components)) = all_consuming(
                    ident_parts_with_components::<nom_supreme::error::ErrorTree<_>>,
                )(package)
                .map_err(|err| match err {
                    nom::Err::Error(e) | nom::Err::Failure(e) => {
                        miette!(
                            "Parsing ident from '{package}' for repo walker failed: {}",
                            e.to_string()
                        )
                    }
                    nom::Err::Incomplete(_) => unreachable!(),
                })?;

                // Set up a filter function for matching the repo, if
                // any, and package name.
                let pkg_name = parts.pkg_name.to_string();
                self.with_package_filter(move |wp| {
                    RepoWalkerFilter::exact_package_name_filter(wp, None, pkg_name.clone())
                });

                // Set up a filter function matching for the version, if any
                if let Some(version_number) = parts.version_str {
                    self.with_version_filter(move |version| {
                        RepoWalkerFilter::exact_match_version_filter(
                            version,
                            version_number.to_string().clone(),
                        )
                    });
                }

                // Set up a filter function for matching the build ident, if any
                if let Some(build_id) = parts.build_str {
                    self.with_build_ident_filter(move |b| {
                        RepoWalkerFilter::exact_match_build_digest_filter(
                            b,
                            build_id.to_string().clone(),
                        )
                    });
                }

                // Set up a filter function for matching the components, if any
                if !components.is_empty() {
                    self.with_component_filter(move |c| {
                        RepoWalkerFilter::allowed_components_filter(c, &components)
                    });
                }
            }
            None => {
                // Leave the filtering functions as they are.
            }
        }
        Ok(self)
    }

    /// Sets package name and version number filters based on the
    /// given version ident. This is a helper function. The same
    /// filters could also be set up by calling:
    /// [Self::with_package_filter] and [Self::with_version_filter].
    pub fn with_version_ident(&mut self, version: VersionIdent) -> &mut Self {
        // Set up a filter function for matching the package name.
        let pkg_name = version.name().to_string();
        self.with_package_filter(move |wp| {
            RepoWalkerFilter::exact_package_name_filter(wp, None, pkg_name.clone())
        });

        // Set up a filter function matching for the version, if any
        let version_number = version.version().clone();
        self.with_version_filter(move |ver| {
            RepoWalkerFilter::exact_match_version_filter(ver, version_number.to_string().clone())
        });

        self
    }

    /// Sets package name, version number, and build id (digest)
    /// filters based on the given build ident. This is a helper
    /// function. The same filters could also be set up by calling:
    /// [Self::with_package_filter], [Self::with_version_filter], and
    /// [Self::with_build_ident_filter].
    pub fn with_build_ident(&mut self, build: BuildIdent) -> &mut Self {
        // Set up a filter function for matching the package name.
        let pkg_name = build.name().to_string();
        self.with_package_filter(move |wp| {
            RepoWalkerFilter::exact_package_name_filter(wp, None, pkg_name.clone())
        });

        // Set up a filter function matching for the version, if any
        let version_number = build.version().clone();
        self.with_version_filter(move |ver| {
            RepoWalkerFilter::exact_match_version_filter(ver, version_number.to_string().clone())
        });

        // Set up a filter function for matching the build ident, if any
        let build_id = build.build().clone();
        self.with_build_ident_filter(move |b| {
            RepoWalkerFilter::exact_match_build_digest_filter(b, build_id.to_string().clone())
        });

        self
    }

    /// Given some file path string, this will use it to set a file
    /// filter function that matches the file path against the start
    /// of each file found by the walker.
    ///
    /// If given None instead of a path string, this will not change
    /// the file filter function.
    ///
    /// This is a helper function used by the spk du command. The same
    /// kind of filter could be setup directly by calling
    /// [Self::with_file_filter].
    pub fn with_file_path(&mut self, file_path: Option<String>) -> &mut Self {
        if let Some(path) = file_path {
            let path_pieces: Vec<String> = path.split("/").map(ToString::to_string).collect();
            self.with_file_filter(move |f| RepoWalkerFilter::parent_paths_match(f, &path_pieces));
        }
        self
    }

    /// Set up a filter function for packages based on their name.
    pub fn with_package_filter(
        &mut self,
        func: impl Fn(&WalkedPackage) -> bool + Send + Sync + 'a,
    ) -> &mut Self {
        self.package_filter_func = Arc::new(func);
        self
    }

    /// Set up a filter function for versions based on their version
    /// number.
    pub fn with_version_filter(
        &mut self,
        func: impl Fn(&WalkedVersion) -> bool + Send + Sync + 'a,
    ) -> &mut Self {
        self.version_filter_func = Arc::new(func);
        self
    }

    /// Set up a filter function for builds based on their build ident
    /// (digest). This is separate from [Self::with_build_spec_filter]
    /// because checking a build's ident is cheaper than reading in
    /// the build's spec to use in filtering.
    pub fn with_build_ident_filter(
        &mut self,
        func: impl Fn(&BuildIdent) -> bool + Send + Sync + 'a,
    ) -> &mut Self {
        self.build_ident_filter_func = Arc::new(func);
        self
    }

    /// Set up a filter function for builds based on their build spec.
    /// This is separate from [Self::with_build_ident_filter] because
    /// reading a build's spec in is more expensive than just checking
    /// a build's ident. But it needed to access some data,
    /// e.g. deprecation status or install requirements.
    pub fn with_build_spec_filter(
        &mut self,
        func: impl Fn(&WalkedBuild) -> bool + Send + Sync + 'a,
    ) -> &mut Self {
        self.build_spec_filter_func = Arc::new(func);
        self
    }

    /// Set up a filter function for components based on the component
    /// name.
    pub fn with_component_filter(
        &mut self,
        func: impl Fn(&WalkedComponent) -> bool + Send + Sync + 'a,
    ) -> &mut Self {
        self.component_filter_func = Arc::new(func);
        self
    }

    /// Set up a filter function for files (dirs and files) based on
    /// the spfs entry and its parent path.
    pub fn with_file_filter(
        &mut self,
        func: impl Fn(&WalkedFile) -> bool + Send + Sync + 'a,
    ) -> &mut Self {
        self.file_filter_func = Arc::new(func);
        self
    }

    /// Have the walk include versions, not just packages. This is
    /// enabled by default.
    pub fn with_report_on_versions(&mut self, report_on_versions: bool) -> &mut Self {
        self.report_on_versions = report_on_versions;
        self
    }

    /// Have the walk include builds, not just packages and versions.
    /// This is enabled by default, but it requires report_on_versions
    /// to be enabled as well or the builds won't be reached.
    pub fn with_report_on_builds(&mut self, report_on_builds: bool) -> &mut Self {
        self.report_on_builds = report_on_builds;
        self
    }

    /// Have the walk include /src builds. This is a global control
    /// that is applied in addition to any build filter function that
    /// is configured. It is disabled by default and /src builds are
    /// filtered out. If you want to show some /src builds and not
    /// others, enable this and specify your custom filtering in one
    /// or more custom build filter functions.
    pub fn with_report_src_builds(&mut self, report_src: bool) -> &mut Self {
        self.report_src = report_src;
        self
    }

    /// Have the walk include deprecated builds. This is a global
    /// control that is applied in addition to any build filter
    /// function is configured. It is disabled by default and
    /// deprecated builds are filtered out. If you want to show some
    /// deprecated builds and not others, enable this and specify your
    /// custom filtering in one or more custom build filter functions.
    pub fn with_report_deprecated_builds(&mut self, report_deprecated: bool) -> &mut Self {
        self.report_deprecated = report_deprecated;
        self
    }

    /// Have the walk include embedded builds. This is a global
    /// control that is applied in addition to any build filter
    /// function is configured. It is enabled by default and embedded
    /// builds are included. If you want to show some embedded builds
    /// and not others, leave this enabled and specify your custom
    /// filtering in one or more custom build filter functions.
    pub fn with_report_embedded_builds(&mut self, report_embedded: bool) -> &mut Self {
        self.report_embedded = report_embedded;
        self
    }

    /// Given some set of build options filters, use them to set up a
    /// build spec filter function to match build's options against.
    /// This is a helper function used by the spk ls, stats and search
    /// commands. The same filter could be set up directly using
    /// [Self::with_build_spec_filter].
    pub fn with_build_options_matching(
        &mut self,
        build_filters: Option<Vec<OptFilter>>,
    ) -> &mut Self {
        if let Some(options_to_have) = build_filters {
            self.with_build_spec_filter(move |s| {
                RepoWalkerFilter::match_build_options_filter(s, options_to_have.clone())
            });
        }
        self
    }

    /// Have the walk include components, not just packages, versions
    /// and builds. This is enabled by default, but it requires
    /// report_on_builds to be enabled as well otherwise the
    /// components won't be reached.
    pub fn with_report_on_components(&mut self, report_on_components: bool) -> &mut Self {
        self.report_on_components = report_on_components;
        self
    }

    /// Have the walk include files, not just packages, versions,
    /// builds, and components. This is enabled by default, but it
    /// requires report_on_components to be enabled as well or the
    /// files won't be reached.
    pub fn with_report_on_files(&mut self, report_on_files: bool) -> &mut Self {
        self.report_on_files = report_on_files;
        self
    }

    /// Have the walk also emit "EndOf" markers when it finishes
    /// walking each object. For example, once everything below a
    /// Version has been processed, all the builds etc., an
    /// EndOfVersion item would be emitted before the next version is
    /// walked. This is disabled by default.
    pub fn with_end_of_markers(&mut self, use_end_of_markers: bool) -> &mut Self {
        self.end_of_markers = use_end_of_markers;
        self
    }

    /// Whether to turn errors into warnings and continue walking from
    /// the next object whenever an error is encountered. This is
    /// disabled by default.
    pub fn with_continue_on_error(&mut self, continue_on_error: bool) -> &mut Self {
        self.continue_on_error = continue_on_error;
        self
    }

    /// Whether to have the walker sort the objects at each level
    /// before emitting them. This is enabled by default. Packages are
    /// sorted by name, versions by highest first, builds by digest,
    /// components by name, and files by name.
    pub fn with_sort_objects(&mut self, sort_objects: bool) -> &mut Self {
        self.sort_objects = sort_objects;
        self
    }

    /// Whether to have the walker only emit the highest version (and
    /// the things beneath it) and then move on to the next package.
    pub fn with_highest_version_only(&mut self, highest_version_only: bool) -> &mut Self {
        self.highest_version_only = highest_version_only;
        self
    }

    /// Whether to calculate if each version is deprecate before emitting it.
    /// This requires processing the version's builds before emitting the version.
    pub fn with_calculate_deprecated_versions(
        &mut self,
        calculate_deprecated_versions: bool,
    ) -> &mut Self {
        self.calculate_deprecated_versions = calculate_deprecated_versions;
        self
    }

    /// Creates a RepoWalker using the builder's current settings.
    pub fn build(&self) -> RepoWalker {
        RepoWalker {
            repos: self.repos,
            package_filter_func: self.package_filter_func.clone(),
            version_filter_func: self.version_filter_func.clone(),
            build_ident_filter_func: self.build_ident_filter_func.clone(),
            build_spec_filter_func: self.build_spec_filter_func.clone(),
            component_filter_func: self.component_filter_func.clone(),
            file_filter_func: self.file_filter_func.clone(),
            report_on_versions: self.report_on_versions,
            report_on_builds: self.report_on_builds,
            report_src: self.report_src,
            report_deprecated: self.report_deprecated,
            report_embedded: self.report_embedded,
            report_on_components: self.report_on_components,
            report_on_files: self.report_on_files,
            emit_end_of_markers: self.end_of_markers,
            continue_on_error: self.continue_on_error,
            sort_objects: self.sort_objects,
            highest_version_only: self.highest_version_only,
            calculate_deprecated_versions: self.calculate_deprecated_versions,
        }
    }
}
