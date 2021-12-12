// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::collections::HashMap;
use std::ffi::OsStr;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use spfs::prelude::Encodable;

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

/// Identifies the source files that should
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuildSource {
    SourcePackage(api::Ident),
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
/// ).unwrap()
/// ```
pub struct BinaryPackageBuilder<'spec> {
    prefix: PathBuf,
    spec: &'spec api::Spec,
    all_options: api::OptionMap,
    source: BuildSource,
    solver: solve::Solver,
    repos: Vec<storage::RepositoryHandle>,
    interactive: bool,
}

impl<'spec> BinaryPackageBuilder<'spec> {
    pub fn from_spec(spec: &'spec api::Spec) -> Self {
        Self {
            spec,
            prefix: PathBuf::from("/spfs"),
            all_options: api::OptionMap::default(),
            source: BuildSource::SourcePackage(spec.pkg.with_build(Some(api::Build::Source))),
            solver: solve::Solver::default(),
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
        self.repos.push(repo);
        self
    }

    pub fn with_repositories(
        &mut self,
        repos: impl IntoIterator<Item = storage::RepositoryHandle>,
    ) -> &mut Self {
        self.repos.extend(repos);
        self
    }

    pub fn set_interactive(&mut self, interactive: bool) -> &mut Self {
        self.interactive = interactive;
        self
    }

    /// Build the requested binary package.
    pub fn build(&mut self) -> Result<api::Spec> {
        let mut runtime = spfs::active_runtime()?;
        runtime.set_editable(true)?;
        runtime.reset_stack()?;
        runtime.reset_all()?;
        spfs::remount_runtime(&runtime)?;

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
        let specs = solution.items();
        let specs = specs
            .iter()
            .map(|solved| &solved.spec)
            .map(std::sync::Arc::as_ref);
        let mut spec = self.spec.clone();
        spec.update_for_build(&self.all_options, specs)?;
        let env = std::env::vars();
        let mut env = solution.to_environment(Some(env));
        env.extend(self.all_options.to_environment());
        let layer = self.build_and_commit_artifacts(env)?;
        storage::local_repository()?.publish_package(spec.clone(), layer.digest()?)?;
        Ok(spec)
    }

    fn resolve_source_package(&mut self, package: &api::Ident) -> Result<solve::Solution> {
        todo!()
        // self._solver.reset()
        // self._solver.update_options(self._all_options)
        // self._solver.add_repository(storage.local_repository())
        // for repo in self._repos:
        //     if repo == storage.local_repository():
        //         # local repo is always injected first, and duplicates are redundant
        //         continue
        //     self._solver.add_repository(repo)

        // if isinstance(self._source, api.Ident):
        //     ident_range = api.parse_ident_range(
        //         f"{self._source.name}/={self._source.version}/{self._source.build}"
        //     )
        //     request = api.PkgRequest(ident_range, "IncludeAll")
        //     self._solver.add_request(request)

        // runtime = solver.run()
        // try:
        //     return runtime.solution()
        // finally:
        //     self._last_solve_graph = runtime.graph()
    }

    fn resolve_build_environment(&mut self) -> Result<solve::Solution> {
        todo!()
        // self._solver.reset()
        // self._solver.update_options(self._all_options)
        // self._solver.set_binary_only(True)
        // for repo in self._repos:
        //     self._solver.add_repository(repo)

        // for request in self.get_build_requirements():
        //     self._solver.add_request(request)

        // runtime = solver.run()
        // try:
        //     return runtime.solution()
        // finally:
        //     self._last_solve_graph = runtime.graph()
    }

    /// List the requirements for the build environment.
    pub fn get_build_requirements(&self) -> Vec<api::Request> {
        todo!()
        // assert (
        //     self._spec is not None
        // ), "Target spec not given, did you use BinaryPackagebuilder.from_spec?"

        // opts = self._spec.resolve_all_options(self._all_options)
        // for opt in self._spec.build.options:
        //     if isinstance(opt, api.PkgOpt):
        //         yield opt.to_request(opts.get(opt.pkg))
        //     elif isinstance(opt, api.VarOpt):
        //         opt_value = opts.get(opt.var)
        //         if not opt_value:
        //             # If no value was specified in the spec, don't
        //             # turn that into a requirement to find that
        //             # var with an empty string value.
        //             continue
        //         yield opt.to_request(opt_value)
        //     else:
        //         raise RuntimeError(f"Unhandled opt type {type(opt)}")
    }

    fn build_and_commit_artifacts<I, K, V>(&mut self, env: I) -> Result<spfs::graph::Layer>
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
    {
        todo!()

        // assert self._spec is not None, "Internal Error: spec is None"

        // self._build_artifacts(env)

        // sources_dir = data_path(self._spec.pkg.with_build(api.SRC), prefix=self._prefix)

        // runtime = spkrs.active_runtime()
        // pattern = os.path.join(sources_dir[len(self._prefix) :], "**")
        // _LOGGER.info("Purging all changes made to source directory", dir=sources_dir)
        // spkrs.reconfigure_runtime(reset=[pattern])

        // _LOGGER.info("Validating package fileset...")
        // try:
        //     self._spec.validate_build_changeset()
        // except RuntimeError as e:
        //     raise BuildError(str(e))

        // return spkrs.commit_layer(runtime)
    }

    fn build_artifacts<I, K, V>(&mut self, env: I) -> Result<()>
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
    {
        todo!()
        // assert self._spec is not None

        // pkg = self._spec.pkg

        // os.makedirs(self._prefix, exist_ok=True)

        // metadata_dir = data_path(pkg, prefix=self._prefix)
        // build_spec = build_spec_path(pkg, prefix=self._prefix)
        // build_options = build_options_path(pkg, prefix=self._prefix)
        // build_script = build_script_path(pkg, prefix=self._prefix)
        // os.makedirs(metadata_dir, exist_ok=True)
        // api.save_spec_file(build_spec, self._spec)
        // with open(build_script, "w+") as writer:
        //     writer.write("\n".join(self._spec.build.script))
        // with open(build_options, "w+") as writer:
        //     json.dump(dict(self._all_options.items()), writer, indent="\t")

        // env.update(self._all_options.to_environment())
        // env.update(get_package_build_env(self._spec))
        // env["PREFIX"] = self._prefix

        // if isinstance(self._source, api.Ident):
        //     source_dir = source_package_path(self._source, self._prefix)
        // else:
        //     source_dir = os.path.abspath(self._source)

        // # force the base environment to be setup using bash, so that the
        // # spfs startup and build environment are predictable and consistent
        // # (eg in case the user's shell does not have startup scripts in
        // #  the dependencies, is not supported by spfs, etc)
        // if self._interactive:
        //     os.environ["SHELL"] = "bash"
        //     print("\nNow entering an interactive build shell")
        //     print(" - your current directory will be set to the sources area")
        //     print(" - build and install your artifacts into /spfs")
        //     print(" - this package's build script can be run from: " + build_script)
        //     print(" - to cancel and discard this build, run `exit 1`")
        //     print(" - to finalize and save the package, run `exit 0`")
        //     cmd = spkrs.build_interactive_shell_command()
        // else:
        //     os.environ["SHELL"] = "bash"
        //     cmd = spkrs.build_shell_initialized_command("bash", "-ex", build_script)
        // with deferred_signals():
        //     proc = subprocess.Popen(cmd, cwd=source_dir, env=env)
        //     proc.wait()
        // if proc.returncode != 0:
        //     raise BuildError(
        //         f"Build script returned non-zero exit status: {proc.returncode}"
        //     )
    }
}

/// Return the environment variables to be set for a build of the given package spec.
pub fn get_package_build_env(spec: &api::Spec) -> HashMap<String, String> {
    let mut env = HashMap::with_capacity(7);
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
pub fn source_package_path<P: AsRef<Path>>(pkg: &api::Ident, prefix: P) -> PathBuf {
    data_path(pkg, prefix)
}

/// Return the file path for the given build's spec.yaml file.
///
/// This file is created during a build and stores the full
/// package spec of what was built.
pub fn build_spec_path<P: AsRef<Path>>(pkg: &api::Ident, prefix: P) -> PathBuf {
    data_path(pkg, prefix).join("spec.yaml")
}

/// Return the file path for the given build's options.json file.
///
/// This file is created during a build and stores the set
/// of build options used when creating the package
pub fn build_options_path<P: AsRef<Path>>(pkg: &api::Ident, prefix: P) -> PathBuf {
    data_path(pkg, prefix).join("options.json")
}

/// Return the file path for the given build's build.sh file.
///
/// This file is created during a build and stores the bash
/// script used to build the package contents
pub fn build_script_path<P: AsRef<Path>>(pkg: &api::Ident, prefix: P) -> PathBuf {
    data_path(pkg, prefix).join("build.sh")
}
