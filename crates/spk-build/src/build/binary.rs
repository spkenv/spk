// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::{hash_map, HashMap, HashSet};
use std::convert::TryInto;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;

use futures::StreamExt;
use relative_path::RelativePathBuf;
use spfs::prelude::*;
use spfs::tracking::{DiffMode, EntryKind};
use spk_exec::{
    pull_resolved_runtime_layers,
    resolve_runtime_layers,
    solution_to_resolved_runtime_layers,
    ResolvedLayer,
};
use spk_schema::foundation::env::data_path;
use spk_schema::foundation::format::FormatIdent;
use spk_schema::foundation::ident_build::Build;
use spk_schema::foundation::ident_component::Component;
use spk_schema::foundation::option_map::OptionMap;
use spk_schema::foundation::version::VERSION_SEP;
use spk_schema::ident::{PkgRequest, PreReleasePolicy, RangeIdent, RequestedBy, VersionIdent};
use spk_schema::version::Compatibility;
use spk_schema::{
    BuildIdent,
    ComponentFileMatchMode,
    ComponentSpecList,
    Package,
    PackageMut,
    Variant,
    VariantExt,
};
use spk_solve::graph::Graph;
use spk_solve::solution::Solution;
use spk_solve::{BoxedResolverCallback, ResolverCallback, Solver};
use spk_storage as storage;
use tokio::pin;

use crate::{Error, Result};

#[cfg(test)]
#[path = "./binary_test.rs"]
mod binary_test;

/// Denotes an error during the build process.
#[derive(Debug, thiserror::Error)]
#[error("Build error: {message}")]
pub struct BuildError {
    pub message: String,
}

impl BuildError {
    pub fn new_error(format_args: std::fmt::Arguments) -> crate::Error {
        crate::Error::Build(Self {
            message: std::fmt::format(format_args),
        })
    }
}

/// Identifies the source files that should be used
/// in a binary package build
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuildSource {
    /// Identifies an existing source package to be resolved
    SourcePackage(RangeIdent),
    /// Specifies that the binary package should be built
    /// against a set of local files.
    ///
    /// Source packages are preferred, but this variant
    /// is useful when rapidly modifying and testing against
    /// a local codebase
    LocalPath(PathBuf),
}

/// A pair of packages that are in conflict for some reason,
/// e.g. because they both provide one or more of the same files.
#[derive(Eq, Hash, PartialEq)]
struct ConflictingPackagePair(BuildIdent, BuildIdent);

/// Builds a binary package.
///
/// ```no_run
/// # use spk_schema::{recipe, foundation::option_map};
/// # async fn demo() {
/// spk_build::BinaryPackageBuilder::from_recipe(recipe!({
///         "pkg": "my-pkg",
///         "build": {"script": "echo hello, world"},
///      }))
///     .build(&option_map!{"debug" => "true"})
///     .await
///     .unwrap();
/// # }
/// ```
pub struct BinaryPackageBuilder<'a, Recipe> {
    prefix: PathBuf,
    recipe: Recipe,
    source: BuildSource,
    solver: Solver,
    environment: HashMap<String, String>,
    source_resolver: BoxedResolverCallback<'a>,
    build_resolver: BoxedResolverCallback<'a>,
    last_solve_graph: Arc<tokio::sync::RwLock<Graph>>,
    repos: Vec<Arc<storage::RepositoryHandle>>,
    interactive: bool,
    files_to_layers: HashMap<RelativePathBuf, ResolvedLayer>,
    conflicting_packages: HashMap<ConflictingPackagePair, HashSet<RelativePathBuf>>,
}

impl<'a, Recipe> BinaryPackageBuilder<'a, Recipe>
where
    Recipe: spk_schema::Recipe,
    Recipe::Output: Package + serde::Serialize,
{
    /// Create a new builder that builds a binary package from the given recipe
    pub fn from_recipe(recipe: Recipe) -> Self {
        let source = BuildSource::SourcePackage(recipe.ident().to_build(Build::Source).into());
        Self {
            recipe,
            source,
            prefix: PathBuf::from("/spfs"),
            solver: Solver::default(),
            environment: Default::default(),
            #[cfg(test)]
            source_resolver: Box::new(spk_solve::DecisionFormatter::new_testing()),
            #[cfg(not(test))]
            source_resolver: Box::new(spk_solve::DefaultResolver {}),
            #[cfg(test)]
            build_resolver: Box::new(spk_solve::DecisionFormatter::new_testing()),
            #[cfg(not(test))]
            build_resolver: Box::new(spk_solve::DefaultResolver {}),
            last_solve_graph: Arc::new(tokio::sync::RwLock::new(Graph::new())),
            repos: Default::default(),
            interactive: false,
            files_to_layers: Default::default(),
            conflicting_packages: Default::default(),
        }
    }

    /// Use an alternate prefix when building (not /spfs).
    ///
    /// This is not something that can usually be done well in a
    /// production context, but can be valuable when testing and
    /// in abnormal circumstances.
    pub fn with_prefix(&mut self, prefix: PathBuf) -> &mut Self {
        self.prefix = prefix;
        self
    }

    /// Define the source files that this build should run against
    pub fn with_source(&mut self, source: BuildSource) -> &mut Self {
        self.source = source;
        self
    }

    /// Use the given repository when resolving source and build environment packages
    pub fn with_repository(&mut self, repo: Arc<storage::RepositoryHandle>) -> &mut Self {
        self.repos.push(repo);
        self
    }

    /// Use the given repositories when resolving source and build environment packages
    pub fn with_repositories(
        &mut self,
        repos: impl IntoIterator<Item = Arc<storage::RepositoryHandle>>,
    ) -> &mut Self {
        self.repos.extend(repos);
        self
    }

    /// Provide a function that will be called when resolving the source package.
    ///
    /// This function should run the provided solver runtime to
    /// completion, returning the final result. This function
    /// is useful for introspecting and reporting on the solve
    /// process as needed.
    pub fn with_source_resolver<F>(&mut self, resolver: F) -> &mut Self
    where
        F: ResolverCallback + 'a,
    {
        self.source_resolver = Box::new(resolver);
        self
    }

    /// Provide a function that will be called when resolving the build environment.
    ///
    /// This function should run the provided solver runtime to
    /// completion, returning the final result. This function
    /// is useful for introspecting and reporting on the solve
    /// process as needed.
    pub fn with_build_resolver<F>(&mut self, resolver: F) -> &mut Self
    where
        F: ResolverCallback + 'a,
    {
        self.build_resolver = Box::new(resolver);
        self
    }

    /// Interactive builds stop just before running the build
    /// script and attempt to spawn an interactive shell process
    /// for the user to inspect and debug the build
    pub fn set_interactive(&mut self, interactive: bool) -> &mut Self {
        self.interactive = interactive;
        self
    }

    /// Return the resolve graph from the build environment.
    ///
    /// This is most useful for debugging build environments that failed to resolve,
    /// and builds that failed with a SolverError.
    ///
    /// If the builder has not run, return an incomplete graph.
    pub fn get_solve_graph(&self) -> Arc<tokio::sync::RwLock<Graph>> {
        self.last_solve_graph.clone()
    }

    pub async fn build_and_publish<V, R, T>(
        &mut self,
        variant: V,
        repo: &R,
    ) -> Result<(Recipe::Output, HashMap<Component, spfs::encoding::Digest>)>
    where
        V: Variant,
        R: std::ops::Deref<Target = T>,
        T: storage::Repository<Recipe = Recipe> + ?Sized,
        <T as storage::Storage>::Package: PackageMut,
    {
        let (package, components) = self.build(variant).await?;
        tracing::debug!("publishing build {}", package.ident().format_ident());
        repo.publish_package(&package, &components).await?;
        Ok((package, components))
    }

    /// Build the requested binary package.
    ///
    /// Returns the unpublished package definition and set of components
    /// layers collected in the local spfs repository.
    pub async fn build<V>(
        &mut self,
        variant: V,
    ) -> Result<(Recipe::Output, HashMap<Component, spfs::encoding::Digest>)>
    where
        V: Variant,
    {
        self.environment.clear();
        let mut runtime = spfs::active_runtime().await?;
        runtime.reset_all()?;
        runtime.status.editable = true;
        runtime.status.stack.clear();

        let requires_localization = runtime.config.mount_backend.requires_localization();

        let variant_options = variant.options();
        tracing::debug!("variant options: {variant_options}");
        let all_options = self.recipe.resolve_options(&variant)?;
        tracing::debug!("  build options: {all_options}");

        if let BuildSource::SourcePackage(ident) = self.source.clone() {
            tracing::debug!("Resolving source package for build");
            let solution = self.resolve_source_package(&all_options, ident).await?;
            runtime
                .status
                .stack
                .extend(resolve_runtime_layers(requires_localization, &solution).await?);
        };

        tracing::debug!("Resolving build environment");
        let solution = self
            .resolve_build_environment(&all_options, &variant)
            .await?;
        self.environment
            .extend(solution.to_environment(Some(std::env::vars())));

        let full_variant = variant
            .with_overrides(solution.options().clone())
            // original options to be reapplied. It feels like this
            // shouldn't be necessary but I've not been able to isolate what
            // goes wrong when this is removed.
            .with_overrides(all_options);

        let resolved_layers = solution_to_resolved_runtime_layers(&solution)?;

        let resolved_layers_copy = resolved_layers.clone();
        let pull_task = if requires_localization {
            tokio::spawn(async move { pull_resolved_runtime_layers(&resolved_layers_copy).await })
        } else {
            tokio::spawn(async move { Ok(resolved_layers_copy.layers()) })
        };

        // Warn about possibly unexpected shadowed files in the layer stack.
        let mut warning_found = false;
        let entries = resolved_layers.iter_entries();
        pin!(entries);

        while let Some(entry) = entries.next().await {
            let (path, entry, resolved_layer) = match entry {
                Err(spk_exec::Error::NonSPFSLayerInResolvedLayers) => continue,
                Err(err) => return Err(err.into()),
                Ok(entry) => entry,
            };

            if !matches!(entry.kind, EntryKind::Blob) {
                continue;
            }
            match self.files_to_layers.entry(path.clone()) {
                hash_map::Entry::Occupied(entry) => {
                    // This file has already been seen by a lower layer.
                    //
                    // Ignore when the shadowing is from different components
                    // of the same package.
                    if entry.get().spec.ident() == resolved_layer.spec.ident() {
                        continue;
                    }
                    // The layer order isn't necessarily meaningful in terms
                    // of spk package dependency ordering (at the time of
                    // writing), so phrase this in a way that doesn't suggest
                    // one layer "owns" the file more than the other.
                    warning_found = true;
                    tracing::warn!(
                        "File {path} found in more than one package: {}:{} and {}:{}",
                        entry.get().spec.ident(),
                        entry.get().component,
                        resolved_layer.spec.ident(),
                        resolved_layer.component,
                    );

                    // Track the packages involved for later use
                    let pkg_a = entry.get().spec.ident().clone();
                    let pkg_b = resolved_layer.spec.ident().clone();
                    let packages_key = if pkg_a < pkg_b {
                        ConflictingPackagePair(pkg_a, pkg_b)
                    } else {
                        ConflictingPackagePair(pkg_b, pkg_a)
                    };
                    let counter = self
                        .conflicting_packages
                        .entry(packages_key)
                        .or_insert_with(HashSet::new);
                    counter.insert(path.clone());
                }
                hash_map::Entry::Vacant(entry) => {
                    // This is the first layer that has this file.
                    entry.insert(resolved_layer.clone());
                }
            };
        }
        if warning_found {
            tracing::warn!("Conflicting files were detected");
            tracing::warn!(" > This can cause undefined runtime behavior");
            tracing::warn!(" > It should be addressed by:");
            tracing::warn!("   - not using these packages together");
            tracing::warn!("   - removing the file from one of them");
            tracing::warn!("   - using alternate versions or components");
        }

        runtime.status.stack.extend(
            pull_task
                .await
                .map_err(|err| Error::String(err.to_string()))??,
        );
        runtime.save_state_to_storage().await?;
        spfs::remount_runtime(&runtime).await?;

        let package = self
            .recipe
            .generate_binary_build(&full_variant, &solution)?;
        self.validate_generated_package(&solution, &package)?;
        let components = self
            .build_and_commit_artifacts(&package, full_variant.options())
            .await?;
        Ok((package, components))
    }

    async fn resolve_source_package(
        &mut self,
        options: &OptionMap,
        package: RangeIdent,
    ) -> Result<Solution> {
        self.solver.reset();
        self.solver.update_options(options.clone());

        let local_repo =
            async { Ok::<_, crate::Error>(Arc::new(storage::local_repository().await?.into())) };

        // If `package` specifies a repository name, only add the
        // repository that matches.
        if let Some(repo_name) = &package.repository_name {
            if repo_name.is_local() {
                self.solver.add_repository(local_repo.await?);
            } else {
                let mut found = false;
                for repo in self.repos.iter() {
                    if repo_name == repo.name() {
                        self.solver.add_repository(repo.clone());
                        found = true;
                        break;
                    }
                }
                if !found {
                    return Err(Error::String(format!(
                        "Repository not found (or enabled) for {package}",
                    )));
                }
            }
        } else {
            // `package` has no opinion about what repo to use.
            let local_repo = local_repo.await?;
            self.solver.add_repository(local_repo.clone());
            for repo in self.repos.iter() {
                if **repo == *local_repo {
                    // local repo is always injected first, and duplicates are redundant
                    continue;
                }
                self.solver.add_repository(repo.clone());
            }
        }

        let source_build = RequestedBy::SourceBuild(package.clone().try_into()?);
        let ident_range = package.with_components([Component::Source]);
        let request = PkgRequest::new(ident_range, source_build)
            .with_prerelease(PreReleasePolicy::IncludeAll)
            .with_pin(None)
            .with_compat(None);

        self.solver.add_request(request.into());

        let (solution, graph) = self.source_resolver.solve(&self.solver).await?;
        self.last_solve_graph = graph;
        Ok(solution)
    }

    async fn resolve_build_environment<V>(
        &mut self,
        options: &OptionMap,
        variant: &V,
    ) -> Result<Solution>
    where
        V: Variant,
    {
        self.solver.reset();
        self.solver.update_options(options.clone());
        self.solver.set_binary_only(true);
        for repo in self.repos.iter().cloned() {
            self.solver.add_repository(repo);
        }

        let build_requirements = self.recipe.get_build_requirements(variant)?.into_owned();
        for request in build_requirements.iter() {
            self.solver.add_request(request.clone());
        }

        let (solution, graph) = self.build_resolver.solve(&self.solver).await?;
        self.last_solve_graph = graph;
        Ok(solution)
    }

    fn validate_generated_package(
        &self,
        solution: &Solution,
        package: &Recipe::Output,
    ) -> Result<()> {
        let build_requirements = package.get_build_requirements()?;
        let runtime_requirements = package.runtime_requirements();
        let solved_packages = solution.items().map(|r| Arc::clone(&r.spec));
        let all_components = package.components();
        for spec in solved_packages {
            for component in all_components.names() {
                let downstream_build = spec.downstream_build_requirements([component]);
                for request in downstream_build.iter() {
                    match build_requirements.contains_request(request) {
                        Compatibility::Compatible => continue,
                        Compatibility::Incompatible(reason) => {
                            return Err(Error::MissingDownstreamBuildRequest {
                                required_by: spec.ident().to_owned(),
                                request: request.clone(),
                                problem: reason.to_string(),
                            })
                        }
                    }
                }
                let downstream_runtime = spec.downstream_runtime_requirements([component]);
                for request in downstream_runtime.iter() {
                    match runtime_requirements.contains_request(request) {
                        Compatibility::Compatible => continue,
                        Compatibility::Incompatible(reason) => {
                            return Err(Error::MissingDownstreamRuntimeRequest {
                                required_by: spec.ident().to_owned(),
                                request: request.clone(),
                                problem: reason.to_string(),
                            })
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Helper for constructing more useful error messages from schema validator errors
    fn assemble_error_message(
        &self,
        error: spk_schema_validators::Error,
        files_to_packages: &HashMap<RelativePathBuf, BuildIdent>,
        conflicting_packages: &HashMap<ConflictingPackagePair, HashSet<RelativePathBuf>>,
    ) -> String {
        match error {
            spk_schema_validators::Error::ExistingFileAltered(diffmode, filepath) => {
                let operation = match *diffmode {
                    DiffMode::Changed(a, b) => {
                        let mut changes: Vec<String> = Vec::new();
                        if a.mode != b.mode {
                            changes.push(format!("permissions: {:06o} => {:06o}", a.mode, b.mode));
                        }
                        if a.kind != b.kind {
                            changes.push(format!("kind: {} => {}", a.kind, b.kind));
                        }
                        if a.object != b.object {
                            changes.push(format!("digest: {} => {}", a.object, b.object));
                        }
                        if a.size != b.size {
                            changes.push(format!("size: {} => {} bytes", a.size, b.size));
                        }

                        format!("Changed [{}]", changes.join(", "))
                    }
                    DiffMode::Removed(_) => String::from("Removed"),
                    _ => String::from("Added or Unchanged"),
                };

                let mut message = format!("\"{}\" was {}", filepath, operation);

                // Work out if the files in conflict came from more
                // than one package
                let packages: Vec<(&ConflictingPackagePair, &HashSet<RelativePathBuf>)> =
                    conflicting_packages
                        .iter()
                        .filter(|(_ps, fs)| fs.contains(&filepath))
                        .collect();

                if packages.is_empty() {
                    // Then the file is only in a single package, not
                    // in a pair of conflicting packages.
                    let package = files_to_packages
                        .get(&filepath)
                        .map(|ident| ident.to_string())
                        .unwrap_or_else(|| {
                            "an unknown package, so something went wrong.".to_string()
                        });
                    message.push_str(&format!(". It is from {package}"));
                } else {
                    let num_others = packages.iter().map(|(_ps, fs)| fs.len()).sum::<usize>() - 1;
                    if num_others > 0 {
                        message.push_str(&format!(
                            " (along with {num_others} more file{})",
                            if num_others == 1 { "" } else { "s" }
                        ));
                    }
                    let pkgs = packages
                        .iter()
                        .flat_map(|(ps, _fs)| Vec::from([ps.0.to_string(), ps.1.to_string()]))
                        .collect::<Vec<String>>();
                    message.push_str(&format!(
                        " in {} packages: {}",
                        pkgs.len(),
                        pkgs.join(" AND ")
                    ));
                }

                message
            }
            _ => error.to_string(),
        }
    }

    async fn build_and_commit_artifacts<O>(
        &mut self,
        package: &Recipe::Output,
        options: O,
    ) -> Result<HashMap<Component, spfs::encoding::Digest>>
    where
        O: AsRef<OptionMap>,
    {
        self.build_artifacts(package, options).await?;

        let source_ident =
            VersionIdent::new(self.recipe.name().to_owned(), self.recipe.version().clone())
                .into_any(Some(Build::Source));
        let sources_dir = data_path(&source_ident);

        let mut runtime = spfs::active_runtime().await?;
        let pattern = sources_dir.join("**").to_string();
        tracing::info!(
            "Purging all changes made to source directory: {}",
            sources_dir.to_path(&self.prefix).display()
        );
        runtime.reset(&[pattern])?;
        runtime.save_state_to_storage().await?;
        spfs::remount_runtime(&runtime).await?;

        tracing::info!("Validating package contents...");

        let changed_files = package
            .validation()
            .validate_build_changeset(package)
            .await
            .map_err(|err| {
                let err_message = match err {
                    spk_schema::Error::InvalidBuildChangeSetError(validator_name, source_err) => {
                        // Simplify this for use during the validation errors and to
                        // avoid having to pass ResolvedLayers down into the validation.
                        let files_to_packages: HashMap<RelativePathBuf, BuildIdent> = self
                            .files_to_layers
                            .iter()
                            .map(|(f, l)| (f.clone(), l.spec.ident().clone()))
                            .collect();

                        format!(
                            "{}: {}",
                            validator_name,
                            self.assemble_error_message(
                                source_err,
                                &files_to_packages,
                                &self.conflicting_packages,
                            )
                        )
                    }
                    _ => format!("{err}"),
                };

                BuildError::new_error(format_args!("Invalid Build: {err_message}"))
            })?;

        tracing::info!("Committing package contents...");
        commit_component_layers(package, &mut runtime, changed_files.as_slice()).await
    }

    async fn build_artifacts<O>(&mut self, package: &Recipe::Output, options: O) -> Result<()>
    where
        O: AsRef<OptionMap>,
    {
        let pkg = package.ident();
        let metadata_dir = data_path(pkg).to_path(&self.prefix);
        let build_spec = build_spec_path(pkg).to_path(&self.prefix);
        let build_options = build_options_path(pkg).to_path(&self.prefix);
        let build_script = build_script_path(pkg).to_path(&self.prefix);

        std::fs::create_dir_all(&metadata_dir)
            .map_err(|err| Error::DirectoryCreateError(metadata_dir.to_owned(), err))?;
        {
            let mut writer = std::fs::File::create(&build_spec)
                .map_err(|err| Error::FileOpenError(build_spec.to_owned(), err))?;
            serde_yaml::to_writer(&mut writer, package)
                .map_err(|err| Error::String(format!("Failed to save build spec: {err}")))?;
            writer
                .sync_data()
                .map_err(|err| Error::FileWriteError(build_spec.to_owned(), err))?;
        }
        {
            let mut writer = std::fs::File::create(&build_script)
                .map_err(|err| Error::FileOpenError(build_script.to_owned(), err))?;
            writer
                .write_all(package.build_script().as_bytes())
                .map_err(|err| Error::String(format!("Failed to save build script: {err}")))?;
            writer
                .sync_data()
                .map_err(|err| Error::FileWriteError(build_script.to_owned(), err))?;
        }
        {
            let mut writer = std::fs::File::create(&build_options)
                .map_err(|err| Error::FileOpenError(build_options.to_owned(), err))?;
            serde_json::to_writer_pretty(&mut writer, options.as_ref())
                .map_err(|err| Error::String(format!("Failed to save build options: {err}")))?;
            writer
                .sync_data()
                .map_err(|err| Error::FileWriteError(build_options.to_owned(), err))?;
        }
        for cmpt in package.components().iter() {
            let marker_path = component_marker_path(pkg, &cmpt.name).to_path(&self.prefix);
            std::fs::File::create(&marker_path)
                .map_err(|err| Error::FileWriteError(marker_path, err))?;
        }

        let source_dir = match &self.source {
            BuildSource::SourcePackage(source) => {
                source_package_path(&source.try_into()?).to_path(&self.prefix)
            }
            BuildSource::LocalPath(path) => path.clone(),
        };

        let runtime = spfs::active_runtime().await?;
        let cmd = if self.interactive {
            println!("\nNow entering an interactive build shell");
            println!(" - your current directory will be set to the sources area");
            println!(" - build and install your artifacts into /spfs");
            println!(
                " - this package's build script can be run from: {}",
                build_script.display()
            );
            println!(" - to cancel and discard this build, run `exit 1`");
            println!(" - to finalize and save the package, run `exit 0`");
            spfs::build_interactive_shell_command(&runtime, Some("bash"))?
        } else {
            use std::ffi::OsString;
            spfs::build_shell_initialized_command(
                &runtime,
                Some("bash"),
                OsString::from("bash"),
                [OsString::from("-ex"), build_script.into_os_string()],
            )?
        };

        let mut cmd = cmd.into_std();
        cmd.envs(self.environment.drain());
        cmd.envs(options.as_ref().to_environment());
        cmd.envs(get_package_build_env(package));
        cmd.env("PREFIX", &self.prefix);
        // force the base environment to be setup using bash, so that the
        // spfs startup and build environment are predictable and consistent
        // (eg in case the user's shell does not have startup scripts in
        //  the dependencies, is not supported by spfs, etc)
        cmd.env("SHELL", "bash");
        cmd.current_dir(&source_dir);

        match cmd
            .status()
            .map_err(|err| {
                Error::ProcessSpawnError(spfs::Error::process_spawn_error(
                    "build script".to_owned(),
                    err,
                    Some(source_dir.to_owned()),
                ))
            })?
            .code()
        {
            Some(0) => (),
            Some(code) => {
                return Err(BuildError::new_error(format_args!(
                    "Build script returned non-zero exit status: {code}"
                )))
            }
            None => {
                return Err(BuildError::new_error(format_args!(
                    "Build script failed unexpectedly"
                )))
            }
        }
        self.generate_startup_scripts(package)
    }

    fn generate_startup_scripts(&self, package: &impl Package) -> Result<()> {
        let ops = package.runtime_environment();
        if ops.is_empty() {
            return Ok(());
        }

        let startup_dir = self.prefix.join("etc").join("spfs").join("startup.d");
        if let Err(err) = std::fs::create_dir_all(&startup_dir) {
            match err.kind() {
                std::io::ErrorKind::AlreadyExists => (),
                _ => return Err(Error::DirectoryCreateError(startup_dir, err)),
            }
        }

        let mut startup_file_csh = startup_dir.join(format!("spk_{}.csh", package.name()));
        let mut startup_file_sh = startup_dir.join(format!("spk_{}.sh", package.name()));
        let mut csh_file = std::fs::File::create(&startup_file_csh)
            .map_err(|err| Error::FileOpenError(startup_file_csh.to_owned(), err))?;
        let mut sh_file = std::fs::File::create(&startup_file_sh)
            .map_err(|err| Error::FileOpenError(startup_file_sh.to_owned(), err))?;
        for op in ops {
            if op.priority().ne(&0) {
                let original_startup_file_sh_name = startup_file_sh.clone();
                let original_startup_file_csh_name = startup_file_csh.clone();

                startup_file_sh.set_file_name(format!(
                    "{}_spk_{}.sh",
                    op.priority(),
                    package.name()
                ));
                startup_file_csh.set_file_name(format!(
                    "{}_spk_{}.csh",
                    op.priority(),
                    package.name()
                ));

                std::fs::rename(original_startup_file_sh_name, &startup_file_sh)
                    .map_err(|err| Error::FileWriteError(startup_file_sh.to_owned(), err))?;
                std::fs::rename(original_startup_file_csh_name, &startup_file_csh)
                    .map_err(|err| Error::FileWriteError(startup_file_csh.to_owned(), err))?;

                continue;
            }

            csh_file
                .write_fmt(format_args!("{}\n", op.tcsh_source()))
                .map_err(|err| Error::FileWriteError(startup_file_csh.to_owned(), err))?;
            sh_file
                .write_fmt(format_args!("{}\n", op.bash_source()))
                .map_err(|err| Error::FileWriteError(startup_file_sh.to_owned(), err))?;
        }
        Ok(())
    }
}

/// Return the environment variables to be set for a build of the given package spec.
pub fn get_package_build_env<P>(spec: &P) -> HashMap<String, String>
where
    P: Package,
{
    let mut env = HashMap::with_capacity(8);
    env.insert("SPK_PKG".to_string(), spec.ident().to_string());
    env.insert("SPK_PKG_NAME".to_string(), spec.name().to_string());
    env.insert("SPK_PKG_VERSION".to_string(), spec.version().to_string());
    env.insert(
        "SPK_PKG_BUILD".to_string(),
        spec.ident().build().to_string(),
    );
    env.insert(
        "SPK_PKG_VERSION_MAJOR".to_string(),
        spec.version().major().to_string(),
    );
    env.insert(
        "SPK_PKG_VERSION_MINOR".to_string(),
        spec.version().minor().to_string(),
    );
    env.insert(
        "SPK_PKG_VERSION_PATCH".to_string(),
        spec.version().patch().to_string(),
    );
    env.insert(
        "SPK_PKG_VERSION_BASE".to_string(),
        spec.version()
            .parts
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(VERSION_SEP),
    );
    env
}

/// Commit changes discovered in the runtime as a package.
///
/// Only the changes also present in `filter` will be committed. It is
/// expected to contain paths relative to `$PREFIX`.
pub async fn commit_component_layers<'a, P>(
    package: &P,
    runtime: &mut spfs::runtime::Runtime,
    filter: impl spfs::tracking::PathFilter + Send + Sync,
) -> Result<HashMap<Component, spfs::encoding::Digest>>
where
    P: Package,
{
    let config = spfs::get_config()?;
    let repo = Arc::new(config.get_local_repository_handle().await?);
    let layer = spfs::Committer::new(&repo)
        .with_path_filter(filter)
        .commit_layer(runtime)
        .await?;
    let manifest = repo
        .read_manifest(layer.manifest)
        .await?
        .to_tracking_manifest();
    let manifests = split_manifest_by_component(package.ident(), &manifest, package.components())?;
    let mut committed = HashMap::with_capacity(manifests.len());
    for (component, manifest) in manifests {
        let manifest = spfs::graph::Manifest::from(&manifest);
        let layer = spfs::graph::Layer {
            manifest: manifest.digest().unwrap(),
        };
        let layer_digest = layer.digest().unwrap();
        #[rustfmt::skip]
        tokio::try_join!(
            async { repo.write_object(&manifest.into()).await },
            async { repo.write_object(&layer.into()).await }
        )?;
        committed.insert(component, layer_digest);
    }
    Ok(committed)
}

fn split_manifest_by_component(
    pkg: &BuildIdent,
    manifest: &spfs::tracking::Manifest,
    components: &ComponentSpecList,
) -> Result<HashMap<Component, spfs::tracking::Manifest>> {
    let mut seen = HashSet::new();
    let mut manifests = HashMap::with_capacity(components.len());
    for component in components.iter() {
        let mut component_manifest = spfs::tracking::Manifest::default();

        // identify all the file paths that we will replicate
        // first so that we can also identify necessary
        // parent directories in a second iteration
        let mut relevant_paths: HashSet<relative_path::RelativePathBuf> = Default::default();
        // all components must include the package metadata
        // as well as the marker file for itself
        relevant_paths.insert(build_spec_path(pkg));
        relevant_paths.insert(build_options_path(pkg));
        relevant_paths.insert(build_script_path(pkg));
        relevant_paths.insert(component_marker_path(pkg, &component.name));
        relevant_paths.extend(path_and_parents(data_path(pkg)));
        for node in manifest.walk() {
            if node.path.strip_prefix(data_path(pkg)).is_ok() {
                // paths within the metadata directory are controlled
                // separately and cannot be included by the component spec
                continue;
            }
            if component
                .files
                .matches(node.path.to_path("/"), node.entry.is_dir())
            {
                let is_new_file = seen.insert(node.path.to_owned());
                if matches!(component.file_match_mode, ComponentFileMatchMode::All) || is_new_file {
                    relevant_paths.extend(path_and_parents(node.path.to_owned()));
                }
            }
        }
        for node in manifest.walk() {
            if relevant_paths.contains(&node.path) {
                tracing::debug!(
                    "{}:{} collecting {:?}",
                    pkg.name(),
                    component.name,
                    node.path
                );
                let mut entry = node.entry.clone();
                if entry.is_dir() {
                    // we will be building back up any directory with
                    // only the children that is should have, so start
                    // with an empty one
                    entry.entries.clear();
                }
                component_manifest.mknod(&node.path, entry)?;
            }
        }

        manifests.insert(component.name.clone(), component_manifest);
    }
    Ok(manifests)
}

/// Return the file path for the given source package's files.
pub fn source_package_path(pkg: &BuildIdent) -> RelativePathBuf {
    data_path(pkg)
}

/// Return the file path for the given build's spec.yaml file.
///
/// This file is created during a build and stores the full
/// package spec of what was built.
pub fn build_spec_path(pkg: &BuildIdent) -> RelativePathBuf {
    data_path(pkg).join("spec.yaml")
}

/// Return the file path for the given build's options.json file.
///
/// This file is created during a build and stores the set
/// of build options used when creating the package
pub fn build_options_path(pkg: &BuildIdent) -> RelativePathBuf {
    data_path(pkg).join("options.json")
}

/// Return the file path for the given build's build.sh file.
///
/// This file is created during a build and stores the bash
/// script used to build the package contents
pub fn build_script_path(pkg: &BuildIdent) -> RelativePathBuf {
    data_path(pkg).join("build.sh")
}

/// Return the file path for the given build's build.sh file.
///
/// This file is created during a build and stores the bash
/// script used to build the package contents
pub fn component_marker_path(pkg: &BuildIdent, name: &Component) -> RelativePathBuf {
    data_path(pkg).join(format!("{name}.cmpt"))
}

/// Expand a path to a list of itself and all of its parents
fn path_and_parents(mut path: RelativePathBuf) -> Vec<RelativePathBuf> {
    let mut hierarchy = Vec::new();
    loop {
        let parent = path.parent().map(ToOwned::to_owned);
        hierarchy.push(path);
        match parent {
            Some(parent) if !parent.as_str().is_empty() => {
                path = parent;
            }
            _ => break,
        }
    }
    hierarchy
}
