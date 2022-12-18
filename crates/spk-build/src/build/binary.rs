// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::{HashMap, HashSet};
use std::convert::TryInto;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;

use itertools::Itertools;
use relative_path::RelativePathBuf;
use spfs::prelude::*;
use spk_exec::resolve_runtime_layers;
use spk_schema::foundation::env::data_path;
use spk_schema::foundation::format::FormatIdent;
use spk_schema::foundation::ident_build::Build;
use spk_schema::foundation::ident_component::Component;
use spk_schema::foundation::option_map::OptionMap;
use spk_schema::foundation::version::VERSION_SEP;
use spk_schema::ident::{PkgRequest, PreReleasePolicy, RangeIdent, RequestedBy, VersionIdent};
use spk_schema::version::Compatibility;
use spk_schema::{
    BuildEnv,
    BuildEnvMember,
    BuildIdent,
    ComponentFileMatchMode,
    ComponentSpecList,
    Package,
    PackageMut,
    RequirementsList,
    Variant,
};
use spk_solve::graph::Graph;
use spk_solve::solution::Solution;
use spk_solve::{BoxedResolverCallback, Request, ResolverCallback, Solver};
use spk_storage::{self as storage};

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

        let variant_options = variant.options();
        tracing::debug!("variant options: {variant_options}");
        let build_options = self.recipe.resolve_options(&variant_options)?;
        tracing::debug!("build options: {build_options}");
        let mut all_options = variant_options.into_owned();
        all_options.extend(build_options.into_iter());

        if let BuildSource::SourcePackage(ident) = self.source.clone() {
            tracing::debug!("Resolving source package for build");
            let solution = self.resolve_source_package(&all_options, ident).await?;
            runtime
                .status
                .stack
                .extend(resolve_runtime_layers(&solution).await?);
        };

        tracing::debug!("Resolving build environment");
        let solution = self
            .resolve_build_environment(&all_options, &variant.additional_requirements())
            .await?;
        self.environment
            .extend(solution.to_environment(Some(std::env::vars())));

        {
            // original options to be reapplied. It feels like this
            // shouldn't be necessary but I've not been able to isolate what
            // goes wrong when this is removed.
            let mut opts = solution.options().clone();
            std::mem::swap(&mut opts, &mut all_options);
            all_options.extend(opts);
        }

        runtime
            .status
            .stack
            .extend(resolve_runtime_layers(&solution).await?);
        runtime.save_state_to_storage().await?;
        spfs::remount_runtime(&runtime).await?;

        let package = self.recipe.generate_binary_build(&solution)?;
        self.validate_generated_package(&solution, &package)?;
        let components = self
            .build_and_commit_artifacts(&package, &all_options)
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

        let mut runtime = self.solver.run();
        let solution = self.source_resolver.solve(&mut runtime).await;
        self.last_solve_graph = runtime.graph();
        Ok(solution?)
    }

    async fn resolve_build_environment(
        &mut self,
        options: &OptionMap,
        additional_requirements: &RequirementsList,
    ) -> Result<Solution> {
        self.solver.reset();
        self.solver.update_options(options.clone());
        self.solver.set_binary_only(true);
        for repo in self.repos.iter().cloned() {
            self.solver.add_repository(repo);
        }

        let build_requirements = self.recipe.get_build_requirements(options)?.into_owned();
        for request in build_requirements.iter() {
            self.solver.add_request(request.clone());
        }
        for mut request in additional_requirements.iter().cloned() {
            if let Request::Pkg(p) = &mut request {
                p.add_requester(RequestedBy::Variant)
            }
            self.solver.add_request(request);
        }

        let mut runtime = self.solver.run();
        let solution = self.build_resolver.solve(&mut runtime).await;
        self.last_solve_graph = runtime.graph();
        Ok(solution?)
    }

    fn validate_generated_package(
        &self,
        // FIXME: how do we handle this because the env has already been resolved...
        solution: &Solution,
        package: &Recipe::Output,
    ) -> Result<()> {
        for solved in solution.members() {
            let spec = solved.package();
            let used_components = solved.used_components();
            let all_components = package.components().names_owned();
            // XXX: do we actually need to cover all combinations here? And if so,
            // would it ever become unreasonably long/slow to check them all?
            for component_mix in all_components.iter().combinations(all_components.len()) {
                let runtime_requirements = package.runtime_requirements(component_mix);
                let downstream_runtime = spec.downstream_requirements(used_components);
                for request in downstream_runtime.iter() {
                    match runtime_requirements.contains_request(request) {
                        Compatibility::Compatible => continue,
                        Compatibility::Incompatible(problem) => {
                            return Err(Error::MissingDownstreamRequest {
                                required_by: spec.ident().to_owned(),
                                request: request.clone(),
                                problem,
                            })
                        }
                    }
                }
            }
        }
        Ok(())
    }

    async fn build_and_commit_artifacts(
        &mut self,
        package: &Recipe::Output,
        options: &OptionMap,
    ) -> Result<HashMap<Component, spfs::encoding::Digest>> {
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
        package
            .validation()
            .validate_build_changeset(package)
            .await
            .map_err(|err| BuildError::new_error(format_args!("{}", err)))?;

        tracing::info!("Committing package contents...");
        commit_component_layers(package, &mut runtime).await
    }

    async fn build_artifacts(
        &mut self,
        package: &Recipe::Output,
        options: &OptionMap,
    ) -> Result<()> {
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
            let script = package.build_script();
            if script.trim().is_empty() {
                return Err(Error::Build(BuildError {
                    message: "package build script was empty".into(),
                }));
            }
            writer
                .write_all(script.as_bytes())
                .map_err(|err| Error::String(format!("Failed to save build script: {}", err)))?;
            writer
                .sync_data()
                .map_err(|err| Error::FileWriteError(build_script.to_owned(), err))?;
        }
        {
            let mut writer = std::fs::File::create(&build_options)
                .map_err(|err| Error::FileOpenError(build_options.to_owned(), err))?;
            serde_json::to_writer_pretty(&mut writer, &options)
                .map_err(|err| Error::String(format!("Failed to save build options: {}", err)))?;
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
                &[OsString::from("-ex"), build_script.into_os_string()],
            )?
        };

        let mut cmd = cmd.into_std();
        cmd.envs(self.environment.drain());
        cmd.envs(options.to_environment());
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
                    "Build script returned non-zero exit status: {}",
                    code
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

        let startup_file_csh = startup_dir.join(format!("spk_{}.csh", package.name()));
        let startup_file_sh = startup_dir.join(format!("spk_{}.sh", package.name()));
        let mut csh_file = std::fs::File::create(&startup_file_csh)
            .map_err(|err| Error::FileOpenError(startup_file_csh.to_owned(), err))?;
        let mut sh_file = std::fs::File::create(&startup_file_sh)
            .map_err(|err| Error::FileOpenError(startup_file_sh.to_owned(), err))?;
        for op in ops {
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

pub async fn commit_component_layers<P>(
    package: &P,
    runtime: &mut spfs::runtime::Runtime,
) -> Result<HashMap<Component, spfs::encoding::Digest>>
where
    P: Package,
{
    let config = spfs::get_config()?;
    let repo = Arc::new(config.get_local_repository_handle().await?);
    let layer = spfs::commit_layer(runtime, Arc::clone(&repo)).await?;
    let manifest = repo.read_manifest(layer.manifest).await?.unlock();
    let manifests = split_manifest_by_component(package.ident(), &manifest, &package.components())?;
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

fn split_manifest_by_component<E>(
    pkg: &BuildIdent,
    manifest: &spfs::tracking::Manifest,
    components: &ComponentSpecList<E>,
) -> Result<HashMap<Component, spfs::tracking::Manifest>>
where
    E: Package,
{
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
                .matches(&node.path.to_path("/"), node.entry.is_dir())
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
    data_path(pkg).join(format!("{}.cmpt", name))
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
