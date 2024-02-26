// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::{HashMap, HashSet};
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
};
use spk_schema::foundation::env::data_path;
use spk_schema::foundation::format::FormatIdent;
use spk_schema::foundation::ident_build::Build;
use spk_schema::foundation::ident_component::Component;
use spk_schema::foundation::option_map::OptionMap;
use spk_schema::foundation::version::VERSION_SEP;
use spk_schema::ident::{PkgRequest, PreReleasePolicy, RangeIdent, RequestedBy, VersionIdent};
use spk_schema::variant::Override;
use spk_schema::{
    BuildIdent,
    ComponentFileMatchMode,
    ComponentSpecList,
    InputVariant,
    Package,
    PackageMut,
    Variant,
    VariantExt,
};
use spk_solve::graph::Graph;
use spk_solve::solution::Solution;
use spk_solve::{BoxedResolverCallback, Named, ResolverCallback, Solver};
use spk_storage as storage;
use tokio::pin;

use crate::report::{BuildOutputReport, BuildReport, BuildSetupReport};
use crate::validation::{Report, Validator};
use crate::{Error, Result};

#[cfg(test)]
#[path = "./binary_test.rs"]
mod binary_test;

/// Denotes an error during the build process.
#[derive(Debug, miette::Diagnostic, thiserror::Error)]
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

/// A struct with the two variants needed to calculate a build digest for a
/// package as well as build the package.
struct VariantPair<V1, V2> {
    input_variant: V1,
    resolved_variant: V2,
}

impl<V1, V2> Variant for VariantPair<V1, V2>
where
    V2: Variant,
{
    #[inline]
    fn options(&self) -> std::borrow::Cow<'_, OptionMap> {
        self.resolved_variant.options()
    }

    #[inline]
    fn additional_requirements(&self) -> std::borrow::Cow<'_, spk_schema::RequirementsList> {
        self.resolved_variant.additional_requirements()
    }
}

impl<V1, V2> InputVariant for VariantPair<V1, V2>
where
    V1: Variant,
    V2: Variant,
{
    type Output = V1;

    #[inline]
    fn input_variant(&self) -> &Self::Output {
        &self.input_variant
    }
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
    conflicting_packages: HashMap<ConflictingPackagePair, HashSet<RelativePathBuf>>,
    allow_circular_dependencies: bool,
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
            conflicting_packages: Default::default(),
            allow_circular_dependencies: false,
        }
    }

    /// Allow circular dependencies when resolving dependencies.
    ///
    /// Normally if a build dependency has a dependency on the package being
    /// built, this is a solver error. But if allow_circular_dependencies is
    /// set to true, this is allowed.
    pub fn with_allow_circular_dependencies(&mut self, allow: bool) -> &mut Self {
        self.allow_circular_dependencies = allow;
        self
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
    ) -> Result<(Recipe::Output, HashMap<Component, spfs::Digest>)>
    where
        V: Variant + Clone + Send + Sync,
        R: std::ops::Deref<Target = T>,
        T: storage::Repository<Recipe = Recipe> + ?Sized,
        <T as storage::Storage>::Package: PackageMut,
    {
        let report = self.build(variant).await?;
        tracing::debug!(
            "publishing build {}",
            report.setup.package.ident().format_ident()
        );
        let components = report
            .output
            .components
            .iter()
            .map(|(n, c)| (n.clone(), c.layer))
            .collect();
        repo.publish_package(&report.setup.package, &components)
            .await?;
        Ok((report.setup.package, components))
    }

    /// Build the requested binary package.
    ///
    /// Returns the unpublished package definition and set of components
    /// layers collected in the local spfs repository.
    pub async fn build<V>(
        &mut self,
        variant: V,
    ) -> Result<BuildReport<Recipe::Output, Override<Override<V>>>>
    where
        V: Variant + Clone + Send + Sync,
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
            .clone()
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

        let mut environment_filesystem = spfs::tracking::Manifest::new(
            // we expect this to be replaced, but the source build for this package
            // seems like one of the most reasonable default owners for the root
            // of this manifest until then
            spfs::tracking::Entry::empty_dir_with_open_perms_with_data(
                self.recipe.ident().to_build(Build::Source),
            ),
        );

        // Warn about possibly unexpected shadowed files in the layer stack.
        let mut warning_found = false;
        let entries = resolved_layers.iter_entries();
        pin!(entries);

        while let Some(entry) = entries.next().await {
            let (path, entry, resolved_layer) = match entry {
                Err(spk_exec::Error::NonSpfsLayerInResolvedLayers) => continue,
                Err(err) => return Err(err.into()),
                Ok(entry) => entry,
            };

            let entry = entry.and_user_data(resolved_layer.spec.ident().to_owned());
            let Some(previous) = environment_filesystem.get_path(&path).cloned() else {
                environment_filesystem.mknod(&path, entry)?;
                continue;
            };
            let entry = environment_filesystem.mknod(&path, entry)?;
            if !matches!(entry.kind, EntryKind::Blob) {
                continue;
            }

            // Ignore when the shadowing is from different components
            // of the same package.
            if entry.user_data == previous.user_data {
                continue;
            }

            // The layer order isn't necessarily meaningful in terms
            // of spk package dependency ordering (at the time of
            // writing), so phrase this in a way that doesn't suggest
            // one layer "owns" the file more than the other.
            warning_found = true;
            tracing::warn!(
                "File {path} found in more than one package: {} and {}",
                previous.user_data,
                entry.user_data
            );

            // Track the packages involved for later use
            let pkg_a = previous.user_data.clone();
            let pkg_b = entry.user_data.clone();
            let packages_key = if pkg_a < pkg_b {
                ConflictingPackagePair(pkg_a, pkg_b)
            } else {
                ConflictingPackagePair(pkg_b, pkg_a)
            };
            let counter = self.conflicting_packages.entry(packages_key).or_default();
            counter.insert(path.clone());
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

        let package = self.recipe.generate_binary_build(
            &VariantPair {
                input_variant: &variant,
                resolved_variant: &full_variant,
            },
            &solution,
        )?;

        // this report will not be complete initially, but the
        // additional functions called after should fill in the
        // final details as the build progresses
        let setup = BuildSetupReport {
            environment: solution,
            package,
            variant: full_variant,
            environment_filesystem,
        };
        let mut report = BuildReport {
            setup,
            // use a default placeholder, assuming it won't be used
            // by the setup validators, and then replaced during the build
            output: Default::default(),
        };
        self.validate_build_setup(&report).await?;
        report.output = self.build_and_commit_artifacts(&report.setup).await?;
        self.validate_build_output(&report).await?;
        Ok(report)
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
            .with_prerelease(Some(PreReleasePolicy::IncludeAll))
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

    async fn validate_build_setup<V>(&self, report: &BuildReport<Recipe::Output, V>) -> Result<()>
    where
        V: Variant + Send + Sync,
    {
        // these must remain ordered so that the overriding of
        // rules is applied correctly later when we merge the results
        let mut validations = futures::stream::FuturesOrdered::new();
        if !report.setup.package.validation().disabled.is_empty() {
            return Err(Error::UseOfObsoleteValidators);
        }
        let validators = report.setup.package.validation().to_expanded_rules();
        tracing::trace!("running validation");
        for validator in validators {
            tracing::trace!(" > {validator:?}");
            validations.push_back(async move { validator.validate_setup(&report.setup).await });
        }
        Report::from_iter(validations.collect::<Vec<_>>().await).into_result()
    }

    async fn validate_build_output<V>(&self, report: &BuildReport<Recipe::Output, V>) -> Result<()>
    where
        V: Variant + Send + Sync,
    {
        // these must remain ordered so that the overriding of
        // rules is applied correctly later when we merge the results
        let mut validations = futures::stream::FuturesOrdered::new();
        let validators = report.setup.package.validation().to_expanded_rules();
        for validator in validators {
            validations.push_back(async move { validator.validate_build(report).await });
        }
        Report::from_iter(validations.collect::<Vec<_>>().await).into_result()
    }

    async fn build_and_commit_artifacts<V: Variant>(
        &mut self,
        input: &BuildSetupReport<Recipe::Output, V>,
    ) -> Result<BuildOutputReport> {
        let options = input.variant.options();
        self.build_artifacts(&input.package, &options).await?;

        let source_ident =
            VersionIdent::new(self.recipe.name().to_owned(), self.recipe.version().clone())
                .into_any(Some(Build::Source));
        let sources_dir = data_path(&source_ident);

        let active_changes = spfs::runtime_active_changes()
            .await?
            .take_root()
            .and_user_data(input.package.ident().to_owned())
            .into();
        let mut collected_changes =
            spfs::tracking::compute_diff(&input.environment_filesystem, &active_changes);
        collected_changes = collected_changes
            .into_iter()
            .filter_map(|diff| {
                // All changes to the sources area are ignored as that is considered to be
                // the build sandbox of the package
                if diff.path.starts_with(&sources_dir) {
                    return None;
                }
                match diff.mode {
                    // Filter out `DiffMode::Removed` entries that aren't `EntryKind::Mask`.
                    // Since we didn't provide the complete manifest for all of /spfs, but
                    // just for the overlayfs upperdir instead, anything that wasn't changed
                    // will show up in the diff as removed.
                    DiffMode::Removed(ref e) if e.kind == spfs::tracking::EntryKind::Mask => {
                        Some(diff)
                    }
                    DiffMode::Removed(_) => None,
                    // Unchanged diffs represent files that were in the working changes/upperdir
                    // but whose contents and permissions were the same as the base layer. In other
                    // words they were touched but not changed. These files are ignored unless
                    // they belong to another version of the package being built. In these cases
                    // we assume that this is a known recursive build, or at least that these files
                    // were written as part of the build but unknowingly clashed with the existing
                    // package. In these cases this type of change would need to be manually reset
                    // during the build script to not be collected.
                    DiffMode::Unchanged(src) if src.user_data.name() != input.package.name() => {
                        None
                    }
                    _ => Some(diff),
                }
            })
            .collect();

        tracing::info!("Committing package contents...");
        commit_component_layers(input, collected_changes).await
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
        cmd.envs(
            options
                .as_ref()
                .to_environment()
                .iter()
                .map(|(k, v)| (&**k, &**v)),
        );
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
                    "build script",
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
            if let Some(priority) = op.priority() {
                let original_startup_file_sh_name = startup_file_sh.clone();
                let original_startup_file_csh_name = startup_file_csh.clone();

                startup_file_sh.set_file_name(format!("{priority:02}_spk_{}.sh", package.name()));
                startup_file_csh.set_file_name(format!("{priority:02}_spk_{}.csh", package.name()));

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
pub async fn commit_component_layers<'a, P, V>(
    input: &BuildSetupReport<P, V>,
    collected_changes: Vec<spfs::tracking::Diff<BuildIdent, BuildIdent>>,
) -> Result<BuildOutputReport>
where
    P: spk_schema::Package,
    V: Variant,
{
    let mut runtime = spfs::active_runtime().await?;
    let config = spfs::get_config()?;
    let repo = Arc::new(config.get_local_repository_handle().await?);
    let layer = spfs::Committer::new(&repo)
        .with_path_filter(collected_changes.as_slice())
        .commit_layer(&mut runtime)
        .await?;
    let collected_layer = repo
        .read_manifest(layer.manifest)
        .await?
        .to_tracking_manifest();
    let manifests = split_manifest_by_component(
        input.package.ident(),
        &collected_layer,
        input.package.components(),
    )?;
    let mut components = HashMap::new();
    for (component, manifest) in manifests {
        let storable_manifest = spfs::graph::Manifest::from(&manifest);
        let layer = spfs::graph::Layer {
            manifest: storable_manifest.digest().unwrap(),
        };
        let layer_digest = layer.digest().unwrap();
        #[rustfmt::skip]
        tokio::try_join!(
            async { repo.write_object(&storable_manifest.into()).await },
            async { repo.write_object(&layer.into()).await }
        )?;
        components.insert(
            component,
            crate::report::BuiltComponentReport {
                layer: layer_digest,
                manifest,
            },
        );
    }
    Ok(BuildOutputReport {
        collected_layer,
        collected_changes,
        components,
    })
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
