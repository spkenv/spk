// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::collections::{HashMap, HashSet};
#[cfg(unix)]
use std::fs::Permissions;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use futures::TryStreamExt;
use itertools::Itertools;
use spfs::prelude::FromUrl;
use spk_schema::foundation::ident_component::Component;
use spk_schema::foundation::name::{PkgName, PkgNameBuf};
use spk_schema::foundation::version::Version;
use spk_schema::ident::VersionIdent;
use spk_schema::name::{OptNameBuf, RepositoryName};
use spk_schema::prelude::Versioned;
use spk_schema::{
    BuildIdent,
    Components,
    Deprecate,
    IndexedPackage,
    OptionValues,
    Package,
    PinnedRequest,
    RequestWithOptions,
    SolverPackageSpec,
    Spec,
    SpecTest,
    build_to_fb_build,
    compat_to_fb_compat,
    component_specs_to_fb_component_specs,
    components_to_fb_components,
    embedded_pkg_specs_to_fb_embedded_package_specs,
    fb_component_names_to_component_names,
    fb_version_to_version,
    flatbuffer_vector,
    get_build_from_fb_build_index,
    opts_to_fb_opts,
    requirements_with_options_to_fb_requirements_with_options,
    version_to_fb_version,
};

use super::repository::Repository;
use crate::storage::{RepositoryIndex, RepositoryIndexMut};
use crate::{Error, RepoWalkerBuilder, RepoWalkerItem, Result};

#[cfg(test)]
#[path = "./flatbuffer_index_test.rs"]
mod flatbuffer_index_test;

// Index schema version supported by spk
const COMPATIBLE_INDEX_SCHEMA_VERSION: u32 = 1;

// Index name and kind constants
pub const FLATBUFFER_INDEX: &str = "flatb";

const INDEX_FILE_PREFIX: &str = "index";
const INDEX_FILE_EXT: &str = "fb";

const INDEX_SUB_DIR: &str = "index";
const SPK_INDEX_SUB_DIR_NAME: &str = "spk";

// Flatbuffer builder constants
const DEFAULT_CAPACITY: usize = 1024;

// Flatbuffer verifier constants
const MAX_FLATBUFFER_TABLES: usize = 100000000;
const DO_NOT_VERIFY: bool = false;
const VERIFY: bool = true;

/// Helper function for removing a file with error conversion
async fn remove_index_file(filepath: &PathBuf) -> Result<()> {
    if let Err(err) = tokio::fs::remove_file(filepath).await {
        match err.kind() {
            std::io::ErrorKind::NotFound => Ok(()),
            _ => Err(Error::String(format!(
                "Unable to remove existing index file '{}' due to: {err}",
                filepath.display()
            ))),
        }
    } else {
        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct FlatBufferRepoIndex {
    // The bytes of the index, usually read from a file
    data_buffer: bytes::Bytes,
}

impl Clone for FlatBufferRepoIndex {
    fn clone(&self) -> Self {
        Self {
            data_buffer: self.data_buffer.clone(),
        }
    }
}
impl FlatBufferRepoIndex {
    /// Verify the flatbuffer data is a valid repository index
    fn check_fb_index(&self) -> Result<spk_proto::RepositoryIndex<'_>> {
        let verifier_opts = flatbuffers::VerifierOptions {
            // The default number of tables seems to be 1 million, and
            // that isn't enough for the current index. Tried 100
            // million and that didn't error, but this is not a great
            // solution long term, especially once VersionFilters are
            // converted from strings.
            max_tables: MAX_FLATBUFFER_TABLES,
            ..Default::default()
        };
        spk_proto::root_as_repository_index_with_opts(&verifier_opts, &self.data_buffer)
            .map_err(|err| Error::String(format!("Error checking flatbuffer index: {err}")))
        // Note: the _unchecked version would be:
        // spk_proto::root_as_repository_index(&self.data_buffer)
        //     .map_err(|err| Error::String(format!("Error checking flatbuffer index: {err}")))
    }

    /// Return the RepositoryIndex data root
    fn fb_index(&self) -> spk_proto::RepositoryIndex<'_> {
        // Assumes check_fb_index was called and the buffer is valid
        unsafe { spk_proto::root_as_repository_index_unchecked(&self.data_buffer) }
    }

    /// Make a flatbuffer repo index from the given bytes
    fn try_from_bytes(
        name: &str,
        data_buffer: bytes::Bytes,
        verify_before_use: bool,
    ) -> Result<Self> {
        let start = Instant::now();
        let index = FlatBufferRepoIndex { data_buffer };
        tracing::debug!(
            "'{name}' repo index flatbuffer to RI struct : {} secs",
            start.elapsed().as_secs_f64()
        );

        // Optionally, verify the flatbuffers data before use
        if verify_before_use {
            let start_check_fb = Instant::now();
            index.check_fb_index()?;
            tracing::debug!(
                "'{name}' repo index checked as flatb RepositoryIndex: {} secs",
                start_check_fb.elapsed().as_secs_f64()
            );
        } else {
            tracing::debug!("'{name}' repo index not verified before use : 0.0 secs");
        }

        // Check the index's schema version to ensure it is compatible
        // with this spk's version.
        if index.fb_index().index_schema_version() != COMPATIBLE_INDEX_SCHEMA_VERSION {
            return Err(Error::String(format!(
                "Index schema is version ({}) is not compatible with spk schema version ({})",
                index.fb_index().index_schema_version(),
                COMPATIBLE_INDEX_SCHEMA_VERSION
            )));
        }

        tracing::debug!(
            "'{name}' repo index flatbuffer total time   : {} secs",
            start.elapsed().as_secs_f64()
        );

        Ok(index)
    }

    /// Create a FlatBufferRepoIndex from an index file in the repository
    pub async fn from_repo_file(repo: &crate::RepositoryHandle) -> Result<FlatBufferRepoIndex> {
        let filepath = Self::repo_index_location(repo).await?;

        let name = repo.name();
        tracing::debug!(
            "Reading repo index file for: '{name}' from filepath: '{}'",
            filepath.display()
        );

        // Based on the configuration setting, decide whether to
        // verify the flatbuffer data before use.
        let config = spk_config::get_config()?;

        FlatBufferRepoIndex::read_index_from_file(
            name,
            &filepath,
            config.solver.indexes.verify_before_use,
        )
        .await
    }

    async fn read_index_from_file(
        name: &RepositoryName,
        filepath: &PathBuf,
        verify_index: bool,
    ) -> Result<FlatBufferRepoIndex> {
        let start = Instant::now();

        // Open and Memory map the data file
        let file = match std::fs::File::open(filepath) {
            Ok(f) => f,
            Err(err) => {
                return Err(Error::IndexOpenError(err));
            }
        };

        let memory_map = match unsafe { memmap2::Mmap::map(&file) } {
            Ok(mm) => mm,
            Err(err) => {
                return Err(Error::IndexMemMapError(err));
            }
        };
        tracing::debug!(
            "'{name}' repo index flatbuffer memmapped in : {} secs",
            start.elapsed().as_secs_f64()
        );

        let start_bytes = Instant::now();
        let data_buffer = bytes::Bytes::from_owner(memory_map);
        tracing::debug!(
            "'{name}' repo index flatbuffer bytes from_ow: {} secs",
            start_bytes.elapsed().as_secs_f64()
        );

        let index = FlatBufferRepoIndex::try_from_bytes(name, data_buffer, verify_index)?;
        Ok(index)
    }

    /// Helper for generating indexes for testing. The index returned
    /// will not have been saved to a file.
    pub async fn from_repo_in_memory(
        repo: &crate::RepositoryHandle,
    ) -> miette::Result<FlatBufferRepoIndex> {
        let start = Instant::now();

        let source_repo: crate::RepositoryHandle = match repo {
            crate::RepositoryHandle::SPFS(_) | crate::RepositoryHandle::Mem(_) => repo.clone(),
            _ => {
                return Err(Error::IndexGenerationInMemError().into());
            }
        };

        let name = repo.name();
        let repos = vec![(repo.name().to_string(), source_repo)];

        let (packages, global_vars) =
            FlatBufferRepoIndex::gather_all_data_from_repo(&repos).await?;

        tracing::info!(
            "'{name}' repo data gathered from memory in: {} secs",
            start.elapsed().as_secs_f64()
        );

        let start_bytes = Instant::now();
        let builder =
            FlatBufferRepoIndex::generate_index_builder(repo, packages, global_vars).await?;

        let bytes = builder.finished_data().to_vec();
        let data_buffer = bytes::Bytes::from(bytes);
        tracing::info!(
            "'{name}' repo index bytes generated in: {} secs",
            start_bytes.elapsed().as_secs_f64()
        );

        // The does not verify the index because we just made it, in
        // memory, and we know it is valid.
        let index = FlatBufferRepoIndex::try_from_bytes(name, data_buffer, DO_NOT_VERIFY)?;

        Ok(index)
    }

    /// Internal method to create a valid Spec from a package held in the
    /// index data as a flatbuffer backed IndexedPackage.
    fn make_solver_package_spec(
        &self,
        package_build: BuildIdent,
        fb_build_index: &spk_proto::BuildIndex,
    ) -> Result<Spec> {
        let build_spec = IndexedPackage::new(
            package_build,
            // This is cheap because it is a bytes::Bytes
            self.data_buffer.clone(),
            fb_build_index._tab.loc(),
        );

        Ok(Spec::V0IndexedPackage(Box::new(build_spec)))
    }

    /// Gather packages and global vars from the given repos.
    async fn gather_all_data_from_repo(
        repos: &Vec<(String, crate::RepositoryHandle)>,
    ) -> miette::Result<(HashMap<PkgNameBuf, PackageInfo>, GlobalVarsInfo)> {
        if repos.len() != 1 {
            return Err(Error::String(
                "FlatBufferRepoIndex's gather_all_data_from_repo() method only works on one repo at a time"
                    .to_string(),
            ).into());
        }

        // Gather all the recipes and builds, including src and deprecated
        let mut repo_walker_builder = RepoWalkerBuilder::new(repos);
        let repo_walker = repo_walker_builder
            .with_report_on_versions(true)
            .with_report_on_builds(true)
            .with_report_src_builds(true)
            .with_report_deprecated_builds(true)
            .with_report_embedded_builds(true)
            .with_end_of_markers(true)
            .with_sort_objects(true)
            .with_continue_on_error(true)
            .build();

        let mut packages: HashMap<PkgNameBuf, PackageInfo> = HashMap::new();
        let mut global_vars = GlobalVarsInfo::default();

        // Needed for filtering global vars and checking the real
        // published components.
        let repo = &repos[0].1;

        let package_names = HashSet::from_iter(repo.list_packages().await?);

        let mut num_versions = 0;
        let mut num_builds = 0;

        let mut traversal = repo_walker.walk();
        while let Some(item) = traversal.try_next().await? {
            match item {
                RepoWalkerItem::Version(version) => {
                    let name = version.ident.name();
                    let v = version.ident.version().clone();

                    let pkg_info = packages.entry(name.into()).or_default();
                    pkg_info.versions.push(v.clone());
                    let _ver_info = pkg_info.version_builds.entry(v).or_default();
                    num_versions += 1;
                }
                RepoWalkerItem::Build(build) => {
                    // Add a build spec and related things
                    let build_ident = build.spec.ident();

                    let pkg_info = packages.entry(build_ident.name().into()).or_default();
                    let ver_info = pkg_info
                        .version_builds
                        .entry(build_ident.version().clone())
                        .or_default();
                    let spec = build.spec.clone();

                    let component_map = match repo.read_components(build.spec.ident()).await {
                        Ok(c) => c,
                        Err(err) => {
                            tracing::warn!(
                                "Problem reading published components for '{}': {err}. Skipping it.",
                                build.spec.ident()
                            );
                            continue;
                        }
                    };
                    let published_components = component_map.keys().cloned().collect();

                    let build_info = BuildInfo {
                        spec,
                        published_components,
                    };
                    ver_info.build_specs.push(build_info);

                    global_vars.extract_global_vars(&build.spec, &package_names)?;

                    num_builds += 1;
                }

                // Ignore everything else
                _ => {}
            }
        }

        // Debugging and logging
        let mut vars: Vec<String> = global_vars.keys().map(|k| k.to_string()).collect();
        vars.sort();
        tracing::info!("Globals found:\n\t{}", vars.into_iter().join("\n\t"));

        tracing::info!(
            "Index for '{}' repo consists of {} packages, {} versions, {} builds, with {} global vars",
            repo.name(),
            packages.len(),
            num_versions,
            num_builds,
            global_vars.keys().len()
        );

        Ok((packages, global_vars))
    }

    /// Internal method to get the current information on the builds
    /// of package version and any global vars they provide. Useful
    /// when updating an existing index.
    async fn gather_updates_from_repo(
        &self,
        repo: &crate::RepositoryHandle,
        package_version: &VersionIdent,
    ) -> miette::Result<(HashMap<PkgNameBuf, PackageInfo>, GlobalVarsInfo)> {
        let start = Instant::now();

        let package_name_to_update = package_version.name().to_owned();
        let arc_version_to_update = Arc::new(package_version.version().clone());

        let mut packages: HashMap<PkgNameBuf, PackageInfo> = HashMap::new();

        let mut num_versions = 0;
        let mut num_builds = 0;

        // Gather the existing global vars. Any new ones from the updated
        // package version will be added when its builds are processed.
        let mut global_vars = GlobalVarsInfo(self.get_global_var_values());

        // Used to work out the order of processing and where to pull
        // in the updates from.
        let mut package_names = self.list_packages().await?;

        // Check update package name and make sure it is in the names list.
        if !package_names.contains(&package_name_to_update) {
            tracing::debug!("'{package_name_to_update}' not in package_names, injecting it",);
            package_names.push(package_name_to_update.clone());
        }

        // Now all the names are present, make a set to help with global variables
        let package_names_set = HashSet::from_iter(package_names.clone());

        // Process the packages, checking for the one to update and
        // pulling from the correct data source for each.
        for name in &package_names {
            let p_v = self.list_package_versions(name).await?;
            let mut package_versions = (*p_v).clone();

            if package_name_to_update == *name && !package_versions.contains(&arc_version_to_update)
            {
                tracing::debug!("{arc_version_to_update} not in package_versions, injecting it",);
                package_versions.push(arc_version_to_update.clone());
                package_versions.sort_by_cached_key(|v| std::cmp::Reverse(v.clone()));
            }

            let pkg_info = packages.entry(name.clone()).or_default();

            for version in package_versions.iter() {
                pkg_info.versions.push((**version).clone());

                let ver_info = pkg_info
                    .version_builds
                    .entry((**version).clone())
                    .or_default();
                num_versions += 1;

                let version_ident = VersionIdent::new(name.clone(), (**version).clone());

                // Check if this is the version we want to update
                if package_name_to_update == *name && arc_version_to_update == *version {
                    tracing::info!("Reached the {version} of the package to update");
                    // Get the updated data from the repo
                    let version_builds = repo.list_package_builds(&version_ident).await?;

                    for build_ident in version_builds {
                        let build_spec = repo.read_package_from_storage(&build_ident).await?;
                        let spec = build_spec.clone();

                        let component_map = repo
                            .read_components_from_storage(build_spec.ident())
                            .await?;
                        let published_components = component_map.keys().cloned().collect();

                        let build_info = BuildInfo {
                            spec,
                            published_components,
                        };

                        ver_info.build_specs.push(build_info);
                        num_builds += 1;

                        // Check the updated build spec for an additional global vars
                        global_vars.extract_global_vars(&build_spec, &package_names_set)?;
                    }
                } else {
                    // Use what's there in the flatbuffer index
                    let version_builds = self.list_package_builds(&version_ident).await?;

                    for build_ident in version_builds {
                        let build_spec = self.get_package_build_spec(&build_ident)?;
                        let spec = build_spec.clone();
                        let published_components = build_spec
                            .components()
                            .iter()
                            .map(|c| c.name.clone())
                            .collect();
                        let build_info = BuildInfo {
                            spec,
                            published_components,
                        };

                        ver_info.build_specs.push(build_info);
                        num_builds += 1;
                    }
                };
            }
        }

        tracing::info!(
            "Updated index data gathered in in: {} secs",
            start.elapsed().as_secs_f64()
        );

        tracing::info!(
            "Index for '{}' repo consists of {} packages, {} versions, {} builds, with {} global vars",
            repo.name(),
            packages.len(),
            num_versions,
            num_builds,
            global_vars.keys().len()
        );

        Ok((packages, global_vars))
    }

    /// Internal method to create a flatbuffers builder for an index
    /// without saving it anywhere.
    async fn generate_index_builder(
        repo: &crate::RepositoryHandle,
        repo_packages: HashMap<PkgNameBuf, PackageInfo>,
        global_vars_info: GlobalVarsInfo,
    ) -> Result<flatbuffers::FlatBufferBuilder<'_>> {
        let start = Instant::now();

        // Gather up the data for the packages field
        let mut packages = Vec::new();
        let mut builder = flatbuffers::FlatBufferBuilder::with_capacity(DEFAULT_CAPACITY);

        let mut package_names: Vec<PkgNameBuf> = repo_packages.keys().map(Clone::clone).collect();
        package_names.sort();

        for name in package_names {
            if let Some(pkg_info) = repo_packages.get(&name) {
                tracing::debug!("package: {name}  [{} versions]", pkg_info.versions.len());
                let package = pkg_info.to_fb_package_index(&mut builder, &name);
                packages.push(package);
            };
        }
        let fb_packages = flatbuffer_vector!(builder, packages);

        // Gather up the global vars field data
        let fb_global_vars = global_vars_info.convert_to_fb(&mut builder);

        // Build the repository index, the root object for the flatbuffer
        let index = spk_proto::RepositoryIndex::create(
            &mut builder,
            &spk_proto::RepositoryIndexArgs {
                index_schema_version: COMPATIBLE_INDEX_SCHEMA_VERSION,
                packages: fb_packages,
                global_vars: fb_global_vars,
            },
        );

        // Finish out the flatbuffer generation
        builder.finish(index, None);

        tracing::info!(
            "flatbuffer index for '{}' assembled in     : {} secs",
            repo.name(),
            start.elapsed().as_secs_f64()
        );

        Ok(builder)
    }

    /// This will create the index path inside the repo, for spk
    /// indexes, if it does not exist.
    async fn get_index_path_from_repo_address(
        repo_name: &str,
        address_url: &url::Url,
    ) -> Result<PathBuf> {
        // Only handles urls that can parse as fs repo configs. Other
        // repository types do not support storing index files.
        let spfs_repo_config = match spfs::storage::fs::Config::from_url(address_url).await {
            Ok(c) => c,
            Err(err) => {
                return Err(Error::IndexNoRepoPathError(
                    repo_name.to_string(),
                    err.to_string(),
                ));
            }
        };

        // TODO: consider making the base index path configurable,
        // with the default being the repo base path + /index/spk.
        let mut index_path = PathBuf::new();
        index_path.push(spfs_repo_config.path);

        index_path.push(INDEX_SUB_DIR);
        spfs::runtime::makedirs_with_perms(&index_path, 0o777).map_err(|source| {
            Error::String(format!(
                "Unable to make '{INDEX_SUB_DIR}' sub-dir in '{repo_name}' repo: {source}"
            ))
        })?;

        index_path.push(SPK_INDEX_SUB_DIR_NAME);
        spfs::runtime::makedirs_with_perms(&index_path, 0o777)
            .map_err(|source| Error::String(format!("Unable to make {SPK_INDEX_SUB_DIR_NAME} sub-dir in '{repo_name}'s index directory: {source}")))?;

        Ok(index_path)
    }

    async fn repo_index_location(repo: &crate::RepositoryHandle) -> Result<PathBuf> {
        let base_path = match repo {
            crate::RepositoryHandle::SPFS(spfs_repo) => {
                Self::get_index_path_from_repo_address(spfs_repo.name(), spfs_repo.address())
                    .await?
            }

            crate::RepositoryHandle::Mem(mem_repo) => {
                // A mem repo doesn't have a usable location for files
                return Err(Error::IndexNoRepoLocationError(
                    mem_repo.name().to_string(),
                    "Spk Mem".to_string(),
                ));
            }

            crate::RepositoryHandle::Runtime(runtime_repo) => {
                // A spfs runtime repo doesn't have a usable location
                // for files.
                return Err(Error::IndexNoRepoLocationError(
                    runtime_repo.name().to_string(),
                    "Spk Runtime".to_string(),
                ));
            }

            crate::RepositoryHandle::Indexed(indexed_repo) => {
                // Indexed repositories are store their index data
                // based on the repo they wrap so use the underlying
                // repo's location for indexes.
                Self::get_index_path_from_repo_address(indexed_repo.name(), indexed_repo.address())
                    .await?
            }
        };

        let mut index_path = PathBuf::new();
        index_path.push(base_path);

        // Index file name contains the index schema version for ease
        // of identifying a compatible index. The index version is
        // also checked later when the bytes are turned into an index
        // in memory.
        index_path.push(format!(
            "{INDEX_FILE_PREFIX}_v{COMPATIBLE_INDEX_SCHEMA_VERSION}.{INDEX_FILE_EXT}",
        ));

        tracing::debug!(
            "{}'s index file location is: {}",
            repo.name(),
            index_path.display()
        );

        Ok(index_path)
    }

    /// Save the index data to a file in the repo. This will save the
    /// index data to a temporary file, read it back in and verify it,
    /// remove the old index file if any, and move the new temp file
    /// into its place.
    async fn save_index<'a>(
        repo: &crate::RepositoryHandle,
        builder: &flatbuffers::FlatBufferBuilder<'a>,
    ) -> Result<PathBuf> {
        let name = repo.name();

        let filepath = Self::repo_index_location(repo).await?;
        let temp_file = PathBuf::from(format!(
            "{}_being_generated_{}",
            filepath.display(),
            ulid::Ulid::new()
        ));

        tracing::debug!("Index file path: {}", filepath.display());
        tracing::debug!("Index temp file: {}", temp_file.display());

        // Create the new index in a temp file with the correct permissions
        if let Err(err) = tokio::fs::write(&temp_file, builder.finished_data()).await {
            // Clean up the temp file after the error
            if let Err(temp_err) = remove_index_file(&temp_file).await {
                tracing::error!("Unable to remove temp index file due to: {temp_err}");
            }
            return Err(Error::IndexWriteError(
                name.to_string(),
                temp_file.display().to_string(),
                err,
            ));
        } else {
            // Ensure the file is readable and writable by everyone
            #[cfg(unix)]
            match tokio::fs::set_permissions(&temp_file, Permissions::from_mode(0o666)).await {
                Err(err) => {
                    // Clean up the temp file after the error
                    if let Err(temp_err) = remove_index_file(&temp_file).await {
                        tracing::error!("Unable to remove temp index file due to: {temp_err}");
                    };
                    Err(Error::FileOpenError(temp_file.clone(), err))
                }
                Ok(ok) => Ok(ok),
            }?;

            // Read in and verify the index
            let _ = FlatBufferRepoIndex::read_index_from_file(name, &temp_file, VERIFY).await?;
        }

        // Delete the old index file, if any. This should not impact
        // existing processes using that index.
        if let Err(err) = remove_index_file(&filepath).await {
            // Clean up the temp file after the error
            if let Err(temp_err) = remove_index_file(&temp_file).await {
                tracing::error!("Unable to remove temp index file due to: {temp_err}");
            }
            return Err(err);
        }

        // Move the index file to the correct place.
        if let Err(err) = tokio::fs::rename(&temp_file, &filepath).await {
            // Clean up the temp file after the error
            if let Err(temp_err) = remove_index_file(&temp_file).await {
                tracing::error!("Unable to remove temp index file due to: {temp_err}");
            }
            return Err(Error::String(format!(
                "Unable to rename new temp index file '{}' to '{}' due to: {err}",
                temp_file.display(),
                filepath.display()
            )));
        }

        Ok(filepath)
    }
}

/// Helper used while generating an index
#[derive(Debug)]
struct BuildInfo {
    spec: Arc<Spec>,
    published_components: Vec<Component>,
}

impl BuildInfo {
    /// Convert the data in a BuildInfo object into the flatbuffers
    /// equivalent, a BuildIndex
    fn to_fb_build_index<'a>(
        &self,
        builder: &mut flatbuffers::FlatBufferBuilder<'a>,
    ) -> flatbuffers::WIPOffset<spk_proto::BuildIndex<'a>> {
        let build_spec = &self.spec;
        let ident = build_spec.ident();

        let fb_build_version = version_to_fb_version(builder, ident.version());

        let (fb_build_id, fb_build_type) = build_to_fb_build(builder, ident.build());

        // Deciding whether to store all the data for a deprecated build.
        // For now, deprecated builds are being fully stored in the
        // index at the same level of detail as non-deprecated packages.
        let store_deprecated_as_placeholders = false;
        //let store_deprecated_as_placeholders = build_spec.is_deprecated();

        let fb_build_index_args = if store_deprecated_as_placeholders {
            let fb_compat = compat_to_fb_compat(builder, &build_spec.compat());

            spk_proto::BuildIndexArgs {
                published_version: Some(fb_build_version),
                build: Some(fb_build_id),
                build_type: fb_build_type,
                is_deprecated: build_spec.is_deprecated(),
                compat: fb_compat,
                // Does not store any more details because the package
                // is deprecated.
                build_options: None,
                runtime_requirements: None,
                embedded: None,
                component_specs: None,
                published_components: None,
            }
        } else {
            let fb_compat = compat_to_fb_compat(builder, &build_spec.compat());

            let fb_build_options = opts_to_fb_opts(builder, &build_spec.get_build_options());

            let fb_requirements = requirements_with_options_to_fb_requirements_with_options(
                builder,
                &build_spec.runtime_requirements(),
            );

            let fb_embedded_specs =
                embedded_pkg_specs_to_fb_embedded_package_specs(builder, &build_spec.embedded());

            let fb_published_components =
                components_to_fb_components(builder, &self.published_components);

            let fb_component_specs =
                component_specs_to_fb_component_specs(builder, &build_spec.components());

            spk_proto::BuildIndexArgs {
                published_version: Some(fb_build_version),
                build: Some(fb_build_id),
                build_type: fb_build_type,
                is_deprecated: build_spec.is_deprecated(),
                compat: fb_compat,
                build_options: fb_build_options,
                runtime_requirements: fb_requirements,
                embedded: fb_embedded_specs,
                component_specs: fb_component_specs,
                published_components: fb_published_components,
            }
        };

        spk_proto::BuildIndex::create(builder, &fb_build_index_args)
    }
}

/// Helper used while generating an index
#[derive(Debug, Default)]
struct VersionInfo {
    build_specs: Vec<BuildInfo>,
}

impl VersionInfo {
    /// Convert a VersionInfo object and a Version number into a
    /// flatbuffers VersionIndex.
    fn to_fb_version_index<'a>(
        &self,
        builder: &mut flatbuffers::FlatBufferBuilder<'a>,
        version: &Version,
    ) -> flatbuffers::WIPOffset<spk_proto::VersionIndex<'a>> {
        let fb_version = version_to_fb_version(builder, version);

        // Get the builds together
        let mut builds = Vec::new();
        for build_info in &self.build_specs {
            let build_index = build_info.to_fb_build_index(builder);
            builds.push(build_index);
        }

        let fb_builds = flatbuffer_vector!(builder, builds);

        // Put this version together
        spk_proto::VersionIndex::create(
            builder,
            &spk_proto::VersionIndexArgs {
                version: Some(fb_version),
                builds: fb_builds,
            },
        )
    }
}

/// Helper used while generating an index
#[derive(Debug, Default)]
struct PackageInfo {
    // To keep the highest to lowest version order from the walker
    versions: Vec<Version>,
    version_builds: HashMap<Version, VersionInfo>,
}

impl PackageInfo {
    /// Convert a PackageInfo object and a name into a flatbuffers
    /// PackageIndex object.
    fn to_fb_package_index<'a>(
        &self,
        builder: &mut flatbuffers::FlatBufferBuilder<'a>,
        name: &PkgNameBuf,
    ) -> flatbuffers::WIPOffset<spk_proto::PackageIndex<'a>> {
        let package_name = builder.create_string(name.as_ref());

        // Get the versions together
        let mut versions = Vec::new();
        for v in &self.versions {
            let version_info = if let Some(ver_info) = self.version_builds.get(v) {
                ver_info
            } else {
                // No builds in this version. So no version data to
                // index, skipping it.
                continue;
            };

            let version_index = version_info.to_fb_version_index(builder, v);
            versions.push(version_index);
        }
        let fb_versions = flatbuffer_vector!(builder, versions);

        // Put the package together
        spk_proto::PackageIndex::create(
            builder,
            &spk_proto::PackageIndexArgs {
                name: Some(package_name),
                versions: fb_versions,
            },
        )
    }
}

/// Helper used while generating an index
#[derive(Clone, Debug, Default)]
pub struct GlobalVarsInfo(HashMap<OptNameBuf, HashSet<String>>);

impl GlobalVarsInfo {
    /// Helper for seeing the names of the vars
    fn keys(&self) -> std::collections::hash_map::Keys<'_, OptNameBuf, HashSet<String>> {
        self.0.keys()
    }

    /// Store a new value for a particular named option. This doesn't
    /// do any checking it assumes the caller did that.
    fn add_new_value(&mut self, name: OptNameBuf, new_value: String) -> &mut Self {
        let entry = self.0.entry(name).or_default();
        entry.insert(new_value);
        self
    }

    /// Extract and store all the global variables and possible values
    /// from the given package build spec. The package names set is
    /// used to filter out package specific variables.
    /// Note: This cannot be run on package build specs from an index
    /// because they don't implement get_all_tests().
    fn extract_global_vars(
        &mut self,
        build_spec: &Spec,
        package_names: &HashSet<PkgNameBuf>,
    ) -> Result<()> {
        // Check the build options for global vars
        for (name, value) in build_spec.option_values() {
            if name.namespace().is_none() {
                let var_name = name.without_namespace().to_owned();
                // Filter out packages
                if package_names.contains(&var_name.to_string()) {
                    continue;
                }
                self.add_new_value(var_name, value);
            }
        }

        // Check install requirements for var requests that could also
        // be global vars.
        for var_req in build_spec
            .runtime_requirements()
            .iter()
            .filter_map(|r| match r {
                RequestWithOptions::Pkg(_p) => None,
                RequestWithOptions::Var(v) => Some(v),
            })
        {
            let var_name = var_req.var.without_namespace().to_owned();
            // Filter out packages
            if package_names.contains(&var_name.to_string()) {
                continue;
            }
            self.add_new_value(var_name, var_req.value.to_string());
        }

        // Requirements from embedded are skipped because they are
        // returned with all the other walked builds.

        // Check the test install requirements
        for test_spec in build_spec.get_all_tests() {
            match test_spec {
                SpecTest::V0(ts) => {
                    ts.requirements
                        .iter()
                        .filter_map(|r| match r {
                            PinnedRequest::Pkg(_p) => None,
                            PinnedRequest::Var(v) => Some(v),
                        })
                        .for_each(|var_req| {
                            let var_name = var_req.var.without_namespace().to_owned();
                            // Filter out variables that use packages names
                            if !package_names.contains(&var_name.to_string()) {
                                self.add_new_value(var_name, var_req.value.to_string());
                            }
                        });
                }
            }
        }

        Ok(())
    }

    /// Adds this global variables data to the given flatbuffer
    /// repository index builder.
    fn convert_to_fb<'a>(
        &self,
        builder: &mut flatbuffers::FlatBufferBuilder<'a>,
    ) -> Option<
        flatbuffers::WIPOffset<
            flatbuffers::Vector<'a, flatbuffers::ForwardsUOffset<spk_proto::GlobalVar<'a>>>,
        >,
    > {
        let mut global_vars = Vec::new();

        let mut var_names: Vec<_> = self.0.keys().collect();
        var_names.sort();

        for name in var_names.into_iter() {
            if let Some(values) = self.0.get(name) {
                let mut sorted_values: Vec<_> = values.iter().collect();
                sorted_values.sort();

                // Create the name and value strings
                let var_name = builder.create_string(name);
                let mut values = Vec::new();
                for v in sorted_values {
                    let val = builder.create_string(v);
                    values.push(val);
                }
                let fb_values = flatbuffer_vector!(builder, values);

                // Create this global var record
                let global_var = spk_proto::GlobalVar::create(
                    builder,
                    &spk_proto::GlobalVarArgs {
                        name: Some(var_name),
                        values: fb_values,
                    },
                );

                global_vars.push(global_var);
            };
        }

        flatbuffer_vector!(builder, global_vars)
    }
}

#[async_trait::async_trait]
impl RepositoryIndexMut for FlatBufferRepoIndex {
    async fn index_repo(repos: &Vec<(String, crate::RepositoryHandle)>) -> miette::Result<()> {
        if repos.len() != 1 {
            return Err(Error::String(
                "FlatBufferRepoIndex's index_repo() method only works on one repo at a time"
                    .to_string(),
            )
            .into());
        }

        let (packages, global_vars) = FlatBufferRepoIndex::gather_all_data_from_repo(repos).await?;

        // Assemble the data into a flatbuffer index and save it
        let repo = &repos[0].1;
        let builder =
            FlatBufferRepoIndex::generate_index_builder(repo, packages, global_vars).await?;

        let start = Instant::now();
        let _filepath = FlatBufferRepoIndex::save_index(repo, &builder).await?;
        tracing::info!(
            "flatbuffer index for '{}' saved in         : {} secs",
            repo.name(),
            start.elapsed().as_secs_f64()
        );

        Ok(())
    }

    // Index and the package version to update within it. The package
    // version to update will have its data gathered from the
    // repository rather than the current index.
    async fn update_repo_with_package_version(
        &self,
        repo: &crate::RepositoryHandle,
        package_version: &VersionIdent,
    ) -> miette::Result<()> {
        let (packages, global_vars) = self.gather_updates_from_repo(repo, package_version).await?;

        // Assemble the data into a flatbuffer index and save it
        let builder =
            FlatBufferRepoIndex::generate_index_builder(repo, packages, global_vars).await?;

        let start = Instant::now();
        let _filepath = FlatBufferRepoIndex::save_index(repo, &builder).await?;
        tracing::info!(
            "flatbuffer index for '{}' saved in: {} secs",
            repo.name(),
            start.elapsed().as_secs_f64()
        );

        Ok(())
    }
}

impl RepositoryIndex for FlatBufferRepoIndex {
    fn get_global_var_values(&self) -> HashMap<OptNameBuf, HashSet<String>> {
        let fb_index = self.fb_index();
        if let Some(globals) = fb_index.global_vars() {
            let mut global_vars: HashMap<OptNameBuf, HashSet<String>> =
                HashMap::with_capacity(globals.len());
            for global in globals {
                let opt_name = unsafe { OptNameBuf::from_string(global.name().to_string()) };
                if let Some(vs) = global.values() {
                    let values: HashSet<String> = vs.iter().map(String::from).collect();
                    global_vars.insert(opt_name, values);
                }
            }
            return global_vars;
        }

        Default::default()
    }

    async fn list_packages(&self) -> Result<Vec<PkgNameBuf>> {
        let fb_index = self.fb_index();
        let pkg_list = if let Some(packages) = fb_index.packages() {
            packages
                .iter()
                .map(|p| unsafe { PkgNameBuf::from_string(p.name().to_string()) })
                .collect()
        } else {
            Default::default()
        };

        Ok(pkg_list)
    }

    async fn list_package_versions(&self, name: &PkgName) -> Result<Arc<Vec<Arc<Version>>>> {
        let fb_index = self.fb_index();
        let versions = if let Some(packages) = fb_index.packages() {
            // binary search - because the name is key in flatbuffer schema
            if let Some(packages) = packages.lookup_by_key(name, |pi, n| pi.name().cmp(n)) {
                if let Some(versions) = packages.versions() {
                    versions
                        .iter()
                        .map(|version_index| {
                            let ver = version_index.version();
                            let version = fb_version_to_version(ver);
                            Arc::new(version)
                        })
                        .collect()
                } else {
                    Vec::new()
                }
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        Ok(Arc::new(versions))
    }

    async fn list_package_builds(&self, pkg: &VersionIdent) -> Result<Vec<BuildIdent>> {
        let fb_index = self.fb_index();
        if let Some(packages) = fb_index.packages()
            && let Some(package) = packages.lookup_by_key(pkg.name(), |pi, n| pi.name().cmp(n))
            && let Some(versions) = package.versions()
        {
            // linear search - versions are highest to lowest, but have
            // lots of parts so that may be the time constraint.
            for version_index in versions {
                let ver = version_index.version();
                let version = fb_version_to_version(ver);

                if version == *pkg.version()
                    && let Some(builds) = version_index.builds()
                    && !builds.is_empty()
                {
                    let build_ids: Vec<_> = builds
                        .iter()
                        .map(|b| {
                            let build_id = get_build_from_fb_build_index(b);
                            BuildIdent::new(pkg.clone(), build_id)
                        })
                        .collect();

                    return Ok(build_ids);
                }
            }
        }

        Ok(Vec::new())
    }

    async fn list_build_components(&self, pkg: &BuildIdent) -> Result<Vec<Component>> {
        let fb_index = self.fb_index();
        if let Some(packages) = fb_index.packages() {
            // binary search - because the name is key in flatbuffer schema
            if let Some(package) = packages.lookup_by_key(pkg.name(), |pi, n| pi.name().cmp(n))
                && let Some(versions) = package.versions()
            {
                // linear search - versions are highest to lowest, but
                // have lots of parts, see above.
                for version_index in versions {
                    let ver = version_index.version();
                    let version = fb_version_to_version(ver);

                    if version == *pkg.version()
                        && let Some(builds) = version_index.builds()
                    {
                        // linear search - the builds aren't ordered
                        for build_index in builds {
                            let build_id = get_build_from_fb_build_index(build_index);
                            if build_id == *pkg.build()
                                && let Some(component_names) = build_index.published_components()
                            {
                                return Ok(fb_component_names_to_component_names(&component_names));
                            }
                        }
                    }
                }
            }
        }

        Ok(Vec::new())
    }

    async fn is_build_deprecated(&self, pkg: &BuildIdent) -> Result<bool> {
        let fb_index = self.fb_index();

        if let Some(packages) = fb_index.packages()
            && let Some(package) = packages.lookup_by_key(pkg.name(), |pi, n| pi.name().cmp(n))
            && let Some(versions) = package.versions()
        {
            // linear search - the versions are highest to lowest, but have lots of parts
            for version_index in versions {
                let ver = version_index.version();
                let version = fb_version_to_version(ver);

                if version == *pkg.version()
                    && let Some(builds) = version_index.builds()
                {
                    // linear search - builds aren't ordered
                    for build_index in builds {
                        let build_id = get_build_from_fb_build_index(build_index);
                        if build_id == *pkg.build() {
                            return Ok(build_index.is_deprecated());
                        }
                    }
                }
            }
        }

        Err(Error::String(format!(
            "{pkg} not found while calling is_build_deprecated"
        )))
    }

    fn get_package_build_spec(&self, pkg: &BuildIdent) -> Result<Arc<Spec>> {
        let fb_index = self.fb_index();
        if let Some(packages) = fb_index.packages()
            && let Some(package) = packages.lookup_by_key(pkg.name(), |pi, n| pi.name().cmp(n))
            && let Some(versions) = package.versions()
        {
            // linear search - versions are highest to lowest, but
            // versions have lots of parts
            for version_index in versions {
                let ver = version_index.version();
                let version = fb_version_to_version(ver);

                if version == *pkg.version()
                    && let Some(builds) = version_index.builds()
                {
                    // linear search - the builds aren't ordered
                    for build_index in builds {
                        let build_id = get_build_from_fb_build_index(build_index);
                        if build_id == *pkg.build() {
                            // Use the build's published version instead of the
                            // version index's version to ensure the build ident
                            // for this build is correct. e.g it may have been
                            // published with a 1.0 version number instead of 1.0.0
                            let package_build = if let Some(pv) = build_index.published_version() {
                                let published_version = fb_version_to_version(pv);
                                let ver_id =
                                    VersionIdent::new(pkg.name().into(), published_version);
                                BuildIdent::new(ver_id, build_id)
                            } else {
                                // This should never happen because the published versions
                                // are stored with each build.
                                pkg.clone()
                            };

                            let build_spec =
                                self.make_solver_package_spec(package_build, &build_index)?;

                            return Ok(Arc::new(build_spec));
                        };
                    }
                }
            }
        }

        Err(Error::PackageNotFound(Box::new(pkg.to_any_ident())))
    }
}
