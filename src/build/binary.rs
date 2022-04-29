// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::Arc;

use relative_path::RelativePathBuf;
use spfs::prelude::*;
use thiserror::Error;

use super::env::data_path;
use crate::solve::Solution;
use crate::{
    api, exec, solve,
    storage::{self, Repository},
    Error, Result,
};

#[cfg(test)]
#[path = "./binary_test.rs"]
mod binary_test;

/// Denotes an error during the build process.
#[derive(Debug, Error)]
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
    SourcePackage(api::Ident),
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
/// # #[macro_use] extern crate spk;
/// # async fn demo() {
/// spk::build::BinaryPackageBuilder::from_spec(spk::spec!({
///         "pkg": "my-pkg",
///         "build": {"script": "echo hello, world"},
///      }))
///     .with_option(spk::opt_name!("debug"), "true")
///     .build()
///     .await
///     .unwrap();
/// # }
/// ```
pub struct BinaryPackageBuilder<'a> {
    prefix: PathBuf,
    spec: api::Spec,
    all_options: api::OptionMap,
    source: BuildSource,
    solver: solve::Solver,
    source_resolver: crate::BoxedResolverCallback<'a>,
    build_resolver: crate::BoxedResolverCallback<'a>,
    last_solve_graph: Arc<tokio::sync::RwLock<solve::Graph>>,
    repos: Vec<Arc<storage::RepositoryHandle>>,
    interactive: bool,
}

impl<'a> BinaryPackageBuilder<'a> {
    /// Create a new builder that builds a binary package from the given spec
    pub fn from_spec(spec: api::Spec) -> Self {
        let source = BuildSource::SourcePackage(spec.pkg.with_build(Some(api::Build::Source)));

        Self {
            spec,
            source,
            prefix: PathBuf::from("/spfs"),
            all_options: api::OptionMap::default(),
            solver: solve::Solver::default(),
            source_resolver: Box::new(crate::DefaultResolver {}),
            build_resolver: Box::new(crate::DefaultResolver {}),
            last_solve_graph: Arc::new(tokio::sync::RwLock::new(solve::Graph::new())),
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

    /// Update a single build option value
    ///
    /// These options are used when computing the final options
    /// for the binary package, and may affect many aspect of the build
    /// environment and generated package.
    pub fn with_option<N, V>(&mut self, name: N, value: V) -> &mut Self
    where
        N: Into<api::OptNameBuf>,
        V: Into<String>,
    {
        self.all_options.insert(name.into(), value.into());
        self
    }

    /// Update the build options with all of the provided ones
    ///
    /// These options are used when computing the final options
    /// for the binary package, and may affect many aspect of the build
    /// environment and generated package.
    pub fn with_options(&mut self, options: api::OptionMap) -> &mut Self {
        self.all_options.extend(options.into_iter());
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
        F: crate::ResolverCallback + 'a,
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
        F: crate::ResolverCallback + 'a,
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
    pub fn get_solve_graph(&self) -> Arc<tokio::sync::RwLock<solve::Graph>> {
        self.last_solve_graph.clone()
    }

    /// Build the requested binary package.
    pub async fn build(&mut self) -> Result<api::Spec> {
        let mut runtime = spfs::active_runtime().await?;
        runtime.reset_all()?;
        runtime.status.editable = true;
        runtime.status.stack.clear();

        let pkg_options = self.spec.resolve_all_options(&self.all_options);
        tracing::debug!("package options: {}", pkg_options);
        let compat = self
            .spec
            .build
            .validate_options(&self.spec.pkg.name, &self.all_options);
        if !&compat {
            return Err(Error::String(compat.to_string()));
        }
        self.all_options.extend(pkg_options);

        let mut stack = Vec::new();
        if let BuildSource::SourcePackage(ident) = self.source.clone() {
            tracing::debug!("Resolving source package for build");
            let solution = self.resolve_source_package(&ident).await?;
            stack.extend(exec::resolve_runtime_layers(&solution).await?)
        };
        tracing::debug!("Resolving build environment");
        let solution = self.resolve_build_environment().await?;
        let mut opts = solution.options();
        std::mem::swap(&mut opts, &mut self.all_options);
        self.all_options.extend(opts);
        stack.extend(exec::resolve_runtime_layers(&solution).await?);
        runtime.status.stack = stack;
        runtime.save_state_to_storage().await?;
        spfs::remount_runtime(&runtime).await?;
        let specs = solution.items().into_iter().map(|solved| solved.spec);
        self.spec.update_for_build(&self.all_options, specs)?;
        let mut env = solution.to_environment(Some(std::env::vars()));
        env.extend(self.all_options.to_environment());
        let components = self.build_and_commit_artifacts(env).await?;
        storage::local_repository()
            .await?
            .publish_package(&self.spec, components)
            .await?;
        Ok(self.spec.clone())
    }

    async fn resolve_source_package(&mut self, package: &api::Ident) -> Result<Solution> {
        self.solver.reset();
        self.solver.update_options(self.all_options.clone());
        let local_repo: Arc<storage::RepositoryHandle> =
            Arc::new(storage::local_repository().await?.into());
        self.solver.add_repository(local_repo.clone());
        for repo in self.repos.iter() {
            if **repo == *local_repo {
                // local repo is always injected first, and duplicates are redundant
                continue;
            }
            self.solver.add_repository(repo.clone());
        }

        let ident_range = api::RangeIdent::equals(package, [api::Component::Source]);
        let request: api::PkgRequest =
            api::PkgRequest::new(ident_range, api::RequestedBy::SourceBuild(package.clone()))
                .with_prerelease(api::PreReleasePolicy::IncludeAll)
                .with_pin(None)
                .with_compat(None);

        self.solver.add_request(request.into());

        let mut runtime = self.solver.run();
        let solution = self.source_resolver.solve(&mut runtime).await;
        self.last_solve_graph = runtime.graph();
        solution
    }

    async fn resolve_build_environment(&mut self) -> Result<Solution> {
        self.solver.reset();
        self.solver.update_options(self.all_options.clone());
        self.solver.set_binary_only(true);
        for repo in self.repos.iter().cloned() {
            self.solver.add_repository(repo);
        }

        for request in self.get_build_requirements()? {
            self.solver.add_request(request);
        }

        let mut runtime = self.solver.run();
        let solution = self.build_resolver.solve(&mut runtime).await;
        self.last_solve_graph = runtime.graph();
        solution
    }

    /// List the requirements for the build environment.
    pub fn get_build_requirements(&self) -> Result<Vec<api::Request>> {
        let opts = self.spec.resolve_all_options(&self.all_options);
        let mut requests = Vec::new();
        for opt in self.spec.build.options.iter() {
            match opt {
                api::Opt::Pkg(opt) => {
                    let given_value = opts.get(opt.pkg.as_opt_name()).map(String::to_owned);
                    let mut req = opt.to_request(
                        given_value,
                        api::RequestedBy::BinaryBuild(self.spec.pkg.clone()),
                    )?;
                    if req.pkg.components.is_empty() {
                        // inject the default component for this context if needed
                        req.pkg
                            .components
                            .insert(api::Component::default_for_build());
                    }
                    requests.push(req.into());
                }
                api::Opt::Var(opt) => {
                    // If no value was specified in the spec, there's
                    // no need to turn that into a requirement to
                    // find a var with an empty value.
                    if let Some(value) = opts.get(&opt.var) {
                        if !value.is_empty() {
                            requests.push(opt.to_request(Some(value)).into());
                        }
                    }
                }
            }
        }
        Ok(requests)
    }

    async fn build_and_commit_artifacts<I, K, V>(
        &mut self,
        env: I,
    ) -> Result<HashMap<api::Component, spfs::encoding::Digest>>
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
    {
        self.build_artifacts(env).await?;

        let sources_dir = data_path(&self.spec.pkg.with_build(Some(api::Build::Source)));

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
        self.spec
            .validate_build_changeset()
            .await
            .map_err(|err| BuildError::new_error(format_args!("{}", err)))?;

        tracing::info!("Committing package contents...");
        commit_component_layers(&self.spec, &mut runtime).await
    }

    async fn build_artifacts<I, K, V>(&mut self, env: I) -> Result<()>
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
    {
        let pkg = &self.spec.pkg;
        let metadata_dir = data_path(pkg).to_path(&self.prefix);
        let build_spec = build_spec_path(pkg).to_path(&self.prefix);
        let build_options = build_options_path(pkg).to_path(&self.prefix);
        let build_script = build_script_path(pkg).to_path(&self.prefix);

        std::fs::create_dir_all(&metadata_dir)?;
        api::save_spec_file(&build_spec, &self.spec)?;
        {
            let mut writer = std::fs::File::create(&build_script)?;
            writer
                .write_all(self.spec.build.script.join("\n").as_bytes())
                .map_err(|err| Error::String(format!("Failed to save build script: {}", err)))?;
            writer.sync_data()?;
        }
        {
            let mut writer = std::fs::File::create(&build_options)?;
            serde_json::to_writer_pretty(&mut writer, &self.all_options)
                .map_err(|err| Error::String(format!("Failed to save build options: {}", err)))?;
            writer.sync_data()?;
        }
        for cmpt in self.spec.install.components.iter() {
            let marker_path = component_marker_path(pkg, &cmpt.name).to_path(&self.prefix);
            std::fs::File::create(marker_path)?;
        }

        let source_dir = match &self.source {
            BuildSource::SourcePackage(source) => source_package_path(source).to_path(&self.prefix),
            BuildSource::LocalPath(path) => path.clone(),
        };

        // force the base environment to be setup using bash, so that the
        // spfs startup and build environment are predictable and consistent
        // (eg in case the user's shell does not have startup scripts in
        //  the dependencies, is not supported by spfs, etc)
        std::env::set_var("SHELL", "bash");
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
            spfs::build_interactive_shell_command(&runtime)?
        } else {
            use std::ffi::OsString;
            spfs::build_shell_initialized_command(
                &runtime,
                OsString::from("bash"),
                &[OsString::from("-ex"), build_script.into_os_string()],
            )?
        };

        let mut cmd = cmd.into_std();
        cmd.envs(env);
        cmd.envs(self.all_options.to_environment());
        cmd.envs(get_package_build_env(&self.spec));
        cmd.env("PREFIX", &self.prefix);
        cmd.current_dir(&source_dir);

        match cmd.status()?.code() {
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
        self.generate_startup_scripts()
    }

    fn generate_startup_scripts(&self) -> Result<()> {
        let ops = &self.spec.install.environment;
        if ops.is_empty() {
            return Ok(());
        }

        let startup_dir = self.prefix.join("etc").join("spfs").join("startup.d");
        if let Err(err) = std::fs::create_dir_all(&startup_dir) {
            match err.kind() {
                std::io::ErrorKind::AlreadyExists => (),
                _ => return Err(err.into()),
            }
        }

        let startup_file_csh = startup_dir.join(format!("spk_{}.csh", self.spec.pkg.name));
        let startup_file_sh = startup_dir.join(format!("spk_{}.sh", self.spec.pkg.name));
        let mut csh_file = std::fs::File::create(startup_file_csh)?;
        let mut sh_file = std::fs::File::create(startup_file_sh)?;
        for op in ops {
            csh_file.write_fmt(format_args!("{}\n", op.tcsh_source()))?;
            sh_file.write_fmt(format_args!("{}\n", op.bash_source()))?;
        }
        Ok(())
    }
}

/// Return the environment variables to be set for a build of the given package spec.
pub fn get_package_build_env(spec: &api::Spec) -> HashMap<String, String> {
    let mut env = HashMap::with_capacity(8);
    env.insert("SPK_PKG".to_string(), spec.pkg.to_string());
    env.insert("SPK_PKG_NAME".to_string(), spec.pkg.name.to_string());
    env.insert("SPK_PKG_VERSION".to_string(), spec.pkg.version.to_string());
    env.insert(
        "SPK_PKG_BUILD".to_string(),
        spec.pkg
            .build
            .as_ref()
            .map(api::Build::to_string)
            .unwrap_or_default(),
    );
    env.insert(
        "SPK_PKG_VERSION_MAJOR".to_string(),
        spec.pkg.version.major().to_string(),
    );
    env.insert(
        "SPK_PKG_VERSION_MINOR".to_string(),
        spec.pkg.version.minor().to_string(),
    );
    env.insert(
        "SPK_PKG_VERSION_PATCH".to_string(),
        spec.pkg.version.patch().to_string(),
    );
    env.insert(
        "SPK_PKG_VERSION_BASE".to_string(),
        spec.pkg
            .version
            .parts
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(api::VERSION_SEP),
    );
    env
}

pub async fn commit_component_layers(
    spec: &api::Spec,
    runtime: &mut spfs::runtime::Runtime,
) -> Result<HashMap<api::Component, spfs::encoding::Digest>> {
    let config = spfs::get_config()?;
    let repo = Arc::new(config.get_local_repository_handle().await?);
    let layer = spfs::commit_layer(runtime, Arc::clone(&repo)).await?;
    let manifest = repo.read_manifest(layer.manifest).await?.unlock();
    let manifests = split_manifest_by_component(&spec.pkg, &manifest, &spec.install.components)?;
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
    pkg: &api::Ident,
    manifest: &spfs::tracking::Manifest,
    components: &api::ComponentSpecList,
) -> Result<HashMap<api::Component, spfs::tracking::Manifest>> {
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
                relevant_paths.extend(path_and_parents(node.path.to_owned()));
            }
        }
        for node in manifest.walk() {
            if relevant_paths.contains(&node.path) {
                tracing::debug!("{}:{} collecting {:?}", pkg.name, component.name, node.path);
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

// Reset all file permissions in spfs if permissions is the
// only change for the given file
// NOTE(rbottriell): permission changes are not properly reset by spfs
// so we must deal with them manually for now
pub fn reset_permissions<P: AsRef<relative_path::RelativePath>>(
    diffs: &mut [spfs::tracking::Diff],
    prefix: P,
) -> Result<()> {
    use spfs::tracking::DiffMode;
    for diff in diffs.iter_mut() {
        match &diff.mode {
            DiffMode::Unchanged(_) | DiffMode::Removed(_) | DiffMode::Added(_) => continue,
            DiffMode::Changed(a, b) => {
                if a.size != b.size {
                    continue;
                }
                if a.object != b.object {
                    continue;
                }
                if a.kind != b.kind {
                    continue;
                }
                let mode_change = a.mode ^ b.mode;
                let nonperm_change = (mode_change | 0o777) ^ 0o777;
                if nonperm_change != 0 {
                    continue;
                }
                if mode_change != 0 {
                    let perms = std::fs::Permissions::from_mode(a.mode);
                    std::fs::set_permissions(
                        diff.path
                            .to_path(PathBuf::from(prefix.as_ref().to_string())),
                        perms,
                    )?;
                }
                diff.mode = DiffMode::Unchanged(a.clone());
            }
        }
    }
    Ok(())
}

/// Return the file path for the given source package's files.
pub fn source_package_path(pkg: &api::Ident) -> RelativePathBuf {
    data_path(pkg)
}

/// Return the file path for the given build's spec.yaml file.
///
/// This file is created during a build and stores the full
/// package spec of what was built.
pub fn build_spec_path(pkg: &api::Ident) -> RelativePathBuf {
    data_path(pkg).join("spec.yaml")
}

/// Return the file path for the given build's options.json file.
///
/// This file is created during a build and stores the set
/// of build options used when creating the package
pub fn build_options_path(pkg: &api::Ident) -> RelativePathBuf {
    data_path(pkg).join("options.json")
}

/// Return the file path for the given build's build.sh file.
///
/// This file is created during a build and stores the bash
/// script used to build the package contents
pub fn build_script_path(pkg: &api::Ident) -> RelativePathBuf {
    data_path(pkg).join("build.sh")
}

/// Return the file path for the given build's build.sh file.
///
/// This file is created during a build and stores the bash
/// script used to build the package contents
pub fn component_marker_path(pkg: &api::Ident, name: &api::Component) -> RelativePathBuf {
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
