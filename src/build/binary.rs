// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use pyo3::prelude::*;
use relative_path::RelativePathBuf;
use spfs::prelude::*;

use super::env::data_path;
use crate::{
    api, exec, solve,
    storage::{self, Repository},
    Error, Result,
};

#[cfg(test)]
#[path = "./binary_test.rs"]
mod binary_test;

/// Denotes an error during the build process.
#[derive(Debug)]
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
/// ```
/// BinaryPackageBuilder
///     .from_spec(api.Spec.from_dict({
///         "pkg": "my-pkg",
///         "build": {"script": "echo hello, world"},
///      }))
///     .with_option("debug", "true")
///     .with_source(".")
///     .build()
///     .unwrap()
/// ```
#[pyclass]
#[derive(Clone)]
pub struct BinaryPackageBuilder {
    prefix: PathBuf,
    spec: api::Spec,
    all_options: api::OptionMap,
    source: BuildSource,
    solver: solve::Solver,
    last_solve_graph: solve::Graph,
    repos: Vec<Arc<Mutex<storage::RepositoryHandle>>>,
    interactive: bool,
}

#[pymethods]
impl BinaryPackageBuilder {
    #[staticmethod]
    pub fn from_spec(spec: api::Spec) -> Self {
        let source = BuildSource::SourcePackage(spec.pkg.with_build(Some(api::Build::Source)));
        Self {
            spec,
            source,
            prefix: PathBuf::from("/spfs"),
            all_options: api::OptionMap::default(),
            solver: solve::Solver::default(),
            last_solve_graph: Default::default(),
            repos: Default::default(),
            interactive: false,
        }
    }
    /// Return the resolve graph from the build environment.
    ///
    /// This is most useful for debugging build environments that failed to resolve,
    /// and builds that failed with a SolverError.
    ///
    /// If the builder has not run, return an incomplete graph.
    pub fn get_solve_graph(&self) -> solve::Graph {
        self.solver.get_last_solve_graph()
    }

    #[pyo3(name = "build")]
    fn build_py(&self) -> Result<api::Spec> {
        // the build function consumes the builder
        // but we cannot represent that in python
        self.clone().build()
    }

    #[pyo3(name = "with_source")]
    fn with_source_py(mut slf: PyRefMut<Self>, source: Py<PyAny>) -> Result<PyRefMut<Self>> {
        if let Ok(ident) = source.extract::<api::Ident>(slf.py()) {
            slf.with_source(BuildSource::SourcePackage(ident));
        } else if let Ok(path) = source.extract::<String>(slf.py()) {
            slf.with_source(BuildSource::LocalPath(path.into()));
        } else {
            return Err(Error::String("Expected api.Ident or str".to_string()));
        }
        Ok(slf)
    }

    #[pyo3(name = "with_option")]
    pub fn with_option_py(mut slf: PyRefMut<Self>, name: String, value: String) -> PyRefMut<Self> {
        slf.with_option(name, value);
        slf
    }

    #[pyo3(name = "with_options")]
    pub fn with_options_py(mut slf: PyRefMut<Self>, options: api::OptionMap) -> PyRefMut<Self> {
        slf.all_options.extend(options.into_iter());
        slf
    }

    #[pyo3(name = "with_repository")]
    pub fn with_repository_py(
        mut slf: PyRefMut<Self>,
        repo: storage::python::Repository,
    ) -> PyRefMut<Self> {
        slf.repos.push(repo.handle);
        slf
    }

    #[pyo3(name = "with_repositories")]
    pub fn with_repositories_py(
        mut slf: PyRefMut<Self>,
        repos: Vec<storage::python::Repository>,
    ) -> PyRefMut<Self> {
        slf.repos.extend(repos.into_iter().map(|r| r.handle));
        slf
    }

    #[pyo3(name = "set_interactive")]
    pub fn set_interactive_py(mut slf: PyRefMut<Self>, interactive: bool) -> PyRefMut<Self> {
        slf.interactive = interactive;
        slf
    }

    #[pyo3(name = "get_build_requirements")]
    pub fn get_build_requirements_py(&self) -> Result<Vec<api::Request>> {
        self.get_build_requirements()
    }
}

impl BinaryPackageBuilder {
    pub fn with_option<N, V>(&mut self, name: N, value: V) -> &mut Self
    where
        N: Into<String>,
        V: Into<String>,
    {
        self.all_options.insert(name.into(), value.into());
        self
    }

    pub fn with_options(&mut self, options: api::OptionMap) -> &mut Self {
        self.all_options.extend(options.into_iter());
        self
    }

    pub fn with_source(&mut self, source: BuildSource) -> &mut Self {
        self.source = source;
        self
    }

    pub fn with_repository(&mut self, repo: storage::RepositoryHandle) -> &mut Self {
        self.repos.push(Arc::new(Mutex::new(repo)));
        self
    }

    pub fn with_repositories(
        &mut self,
        repos: impl IntoIterator<Item = storage::RepositoryHandle>,
    ) -> &mut Self {
        self.repos
            .extend(repos.into_iter().map(Mutex::new).map(Arc::new));
        self
    }

    pub fn set_interactive(&mut self, interactive: bool) -> &mut Self {
        self.interactive = interactive;
        self
    }

    /// Build the requested binary package.
    pub fn build(mut self) -> Result<api::Spec> {
        let mut runtime = spfs::active_runtime()?;
        runtime.set_editable(true)?;
        runtime.reset_all()?;
        runtime.reset_stack()?;

        let pkg_options = self.spec.resolve_all_options(&self.all_options);
        tracing::debug!("package options: {}", pkg_options);
        let compat = self
            .spec
            .build
            .validate_options(self.spec.pkg.name(), &self.all_options);
        if !&compat {
            return Err(Error::String(compat.to_string()));
        }
        self.all_options.extend(pkg_options);

        let mut stack = Vec::new();
        if let BuildSource::SourcePackage(ident) = self.source.clone() {
            let solution = self.resolve_source_package(&ident)?;
            stack.extend(exec::resolve_runtime_layers(&solution)?);
        };
        let solution = self.resolve_build_environment()?;
        let mut opts = solution.options();
        std::mem::swap(&mut opts, &mut self.all_options);
        self.all_options.extend(opts);
        stack.extend(exec::resolve_runtime_layers(&solution)?);
        for digest in stack.into_iter() {
            runtime.push_digest(&digest)?;
        }
        spfs::remount_runtime(&runtime)?;
        let specs = solution.items();
        let specs = specs
            .iter()
            .map(|solved| &solved.spec)
            .map(std::sync::Arc::as_ref);
        self.spec.update_for_build(&self.all_options, specs)?;
        let env = std::env::vars();
        let mut env = solution.to_environment(Some(env));
        env.extend(self.all_options.to_environment());
        let components = self.build_and_commit_artifacts(env)?;
        storage::local_repository()?.publish_package(self.spec.clone(), components)?;
        Ok(self.spec)
    }

    fn resolve_source_package(&mut self, package: &api::Ident) -> Result<solve::Solution> {
        self.solver.reset();
        self.solver.update_options(self.all_options.clone());
        let local_repo = Arc::new(Mutex::new(storage::local_repository()?.into()));
        self.solver.add_repository(local_repo.clone());
        for repo in self.repos.iter() {
            if *repo.lock().unwrap() == *local_repo.lock().unwrap() {
                // local repo is always injected first, and duplicates are redundant
                continue;
            }
            self.solver.add_repository(repo.clone());
        }

        let ident_range = api::RangeIdent::exact(package, [api::Component::Source]);
        let request = api::PkgRequest {
            pkg: ident_range,
            prerelease_policy: api::PreReleasePolicy::IncludeAll,
            inclusion_policy: api::InclusionPolicy::Always,
            pin: None,
            required_compat: None,
        };
        self.solver.add_request(request.into());

        let mut runtime = self.solver.run();
        let solution = runtime.solution();
        self.last_solve_graph = runtime.graph().read().unwrap().clone();
        Ok(solution?)
    }

    fn resolve_build_environment(&mut self) -> Result<solve::Solution> {
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
        let solution = runtime.solution();
        self.last_solve_graph = runtime.graph().read().unwrap().clone();
        Ok(solution?)
    }

    /// List the requirements for the build environment.
    pub fn get_build_requirements(&self) -> Result<Vec<api::Request>> {
        let opts = self.spec.resolve_all_options(&self.all_options);
        let mut requests = Vec::new();
        for opt in self.spec.build.options.iter() {
            match opt {
                api::Opt::Pkg(opt) => {
                    let given_value = opts.get(&opt.pkg).map(String::to_owned);
                    let mut req = opt.to_request(given_value)?;
                    if req.pkg.components.is_empty() {
                        // inject the default component for this context if needed
                        req.pkg.components.insert(api::Component::Build);
                    }
                    requests.push(req.into());
                }
                api::Opt::Var(opt) => {
                    // If no value was specified in the spec, there's
                    // no need to turn that into a requirement to
                    // find a var with an empty value.
                    if let Some(value) = opts.get(&opt.var) {
                        requests.push(opt.to_request(Some(value)).into());
                    }
                }
            }
        }
        Ok(requests)
    }

    fn build_and_commit_artifacts<I, K, V>(
        &mut self,
        env: I,
    ) -> Result<HashMap<api::Component, spfs::encoding::Digest>>
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
    {
        self.build_artifacts(env)?;

        let sources_dir = data_path(&self.spec.pkg.with_build(Some(api::Build::Source)));

        let mut runtime = spfs::active_runtime()?;
        let pattern = sources_dir.join("**").to_string();
        tracing::info!(
            "Purging all changes made to source directory: {}",
            sources_dir.to_path(&self.prefix).display()
        );
        runtime.reset(&[pattern])?;
        spfs::remount_runtime(&runtime)?;

        tracing::info!("Validating package fileset...");
        self.spec
            .validate_build_changeset()
            .map_err(|err| BuildError::new_error(format_args!("{}", err)))?;

        commit_component_layers(&self.spec, &mut runtime)
    }

    fn build_artifacts<I, K, V>(&mut self, env: I) -> Result<()>
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
        let cmd = if self.interactive {
            let runtime = spfs::active_runtime()?;
            println!("\nNow entering an interactive build shell");
            println!(" - your current directory will be set to the sources area");
            println!(" - build and install your artifacts into /spfs");
            println!(
                " - this package's build script can be run from: {}",
                build_script.display()
            );
            println!(" - to cancel and discard this build, run `exit 1`");
            println!(" - to finalize and save the package, run `exit 0`");
            spfs::build_interactive_shell_cmd(&runtime)?
        } else {
            use std::ffi::OsString;
            spfs::build_shell_initialized_command(
                OsString::from("bash"),
                &mut vec![OsString::from("-ex"), build_script.into_os_string()],
            )?
        };

        let mut args = cmd.into_iter();
        let mut cmd = std::process::Command::new(args.next().unwrap());
        cmd.args(args);
        cmd.envs(env);
        cmd.envs(self.all_options.to_environment());
        cmd.envs(get_package_build_env(&self.spec));
        cmd.env("PREFIX", &self.prefix);
        cmd.current_dir(&source_dir);

        match cmd.status() {
            Err(err) => Err(err.into()),
            Ok(status) => match status.code() {
                Some(0) => Ok(()),
                Some(code) => Err(BuildError::new_error(format_args!(
                    "Build script returned non-zero exit status: {}",
                    code
                ))),
                None => Err(BuildError::new_error(format_args!(
                    "Build script failed unexpectedly"
                ))),
            },
        }
    }
}

/// Return the environment variables to be set for a build of the given package spec.
pub fn get_package_build_env(spec: &api::Spec) -> HashMap<String, String> {
    let mut env = HashMap::with_capacity(8);
    env.insert("SPK_PKG".to_string(), spec.pkg.to_string());
    env.insert("SPK_PKG_NAME".to_string(), spec.pkg.name().to_string());
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
        spec.pkg.version.major.to_string(),
    );
    env.insert(
        "SPK_PKG_VERSION_MINOR".to_string(),
        spec.pkg.version.minor.to_string(),
    );
    env.insert(
        "SPK_PKG_VERSION_PATCH".to_string(),
        spec.pkg.version.patch.to_string(),
    );
    env.insert(
        "SPK_PKG_VERSION_BASE".to_string(),
        spec.pkg
            .version
            .parts()
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(api::VERSION_SEP),
    );
    env
}

pub fn commit_component_layers(
    spec: &api::Spec,
    runtime: &mut spfs::runtime::Runtime,
) -> Result<HashMap<api::Component, spfs::encoding::Digest>> {
    let layer = spfs::commit_layer(runtime)?;
    let config = spfs::load_config()?;
    let mut repo = config.get_repository()?;
    let manifest = repo.read_manifest(&layer.manifest)?.unlock();
    let manifests = split_manifest_by_component(&spec.pkg, &manifest, &spec.install.components)?;
    manifests
        .into_iter()
        .map(|(name, m)| (name, spfs::graph::Manifest::from(&m)))
        .map(|(name, m)| {
            let layer = spfs::graph::Layer {
                manifest: m.digest().unwrap(),
            };
            let layer_digest = layer.digest().unwrap();
            repo.write_object(&m.into())
                .and_then(|_| repo.write_object(&layer.into()))
                .map_err(crate::Error::from)
                .map(|_| (name, layer_digest))
        })
        .collect()
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

// Reset all file permissions in spfs if permissions is the
// only change for the given file
// NOTE(rbottriell): permission changes are not properly reset by spfs
// so we must deal with them manually for now
pub fn reset_permissions<P: AsRef<relative_path::RelativePath>>(
    diffs: &mut Vec<spfs::tracking::Diff>,
    prefix: P,
) -> Result<()> {
    for diff in diffs.iter_mut() {
        if diff.mode != spfs::tracking::DiffMode::Changed {
            continue;
        }
        if let Some((a, b)) = &diff.entries {
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
            let nonperm_change = (mode_change | 0o777) ^ 0o77;
            if mode_change != 0 && nonperm_change == 0 {
                let perms = std::fs::Permissions::from_mode(a.mode);
                std::fs::set_permissions(
                    diff.path
                        .to_path(PathBuf::from(prefix.as_ref().to_string())),
                    perms,
                )?;
                diff.mode = spfs::tracking::DiffMode::Unchanged;
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
///
/// ```
/// use relative_path::RelativePathBuf;
/// let path = RelativePathBuf::from("some/deep/path")
/// let hierarchy = path_and_parents(path);
/// assert_eq!(hierarchy, vec![
///     RelativePathBuf::from("some/deep/path"),
///     RelativePathBuf::from("some/deep"),
///     RelativePathBuf::from("some"),
/// ]);
/// ```
fn path_and_parents(mut path: RelativePathBuf) -> Vec<RelativePathBuf> {
    let mut hierarchy = Vec::new();
    loop {
        let parent = path.parent().map(ToOwned::to_owned);
        hierarchy.push(path);
        match parent {
            None => break,
            Some(parent) => {
                path = parent;
            }
        }
    }
    hierarchy
}
