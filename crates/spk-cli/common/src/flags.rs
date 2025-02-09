// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

mod variant;

use std::collections::HashSet;
use std::convert::From;
use std::sync::Arc;

use clap::{Args, ValueEnum, ValueHint};
use miette::{Context, IntoDiagnostic, Result, bail, miette};
use solve::{
    DEFAULT_SOLVER_RUN_FILE_PREFIX,
    DecisionFormatter,
    DecisionFormatterBuilder,
    MultiSolverKind,
    Solver as SolverTrait,
};
use spk_schema::foundation::format::FormatIdent;
use spk_schema::foundation::ident_build::Build;
use spk_schema::foundation::ident_component::Component;
use spk_schema::foundation::name::OptName;
use spk_schema::foundation::option_map::OptionMap;
use spk_schema::foundation::version::CompatRule;
use spk_schema::ident::{
    AnyIdent,
    AsVersionIdent,
    PkgRequest,
    RangeIdent,
    Request,
    RequestedBy,
    VarRequest,
    parse_ident,
};
use spk_schema::option_map::HOST_OPTIONS;
use spk_schema::{Recipe, SpecFileData, SpecRecipe, Template, TestStage, VariantExt};
#[cfg(feature = "statsd")]
use spk_solve::{SPK_RUN_TIME_METRIC, get_metrics_client};
use spk_workspace::{FindOrLoadPackageTemplateError, FindPackageTemplateError};
pub use variant::{Variant, VariantBuildStatus, VariantLocation};
use {spk_solve as solve, spk_storage as storage};

use crate::parsing::{VariantIndex, stage_specifier};
use crate::{CommandArgs, Error};

#[cfg(test)]
#[path = "./flags_test.rs"]
mod flags_test;

static SPK_NO_RUNTIME: &str = "SPK_NO_RUNTIME";
static SPK_KEEP_RUNTIME: &str = "SPK_KEEP_RUNTIME";
static SPK_SOLVER_OUTPUT_TO_DIR: &str = "SPK_SOLVER_OUTPUT_TO_DIR";
static SPK_SOLVER_OUTPUT_TO_DIR_MIN_VERBOSITY: &str = "SPK_SOLVER_OUTPUT_TO_DIR_MIN_VERBOSITY";
static SPK_SOLVER_OUTPUT_FILE_PREFIX: &str = "SPK_SOLVER_OUTPUT_FILE_PREFIX";

#[derive(Args, Clone)]
pub struct Runtime {
    /// Reconfigure the current spfs runtime (useful for speed and debugging)
    #[clap(long, env = SPK_NO_RUNTIME)]
    pub no_runtime: bool,

    /// Make the underlying /spfs filesystem editable
    #[clap(long)]
    pub edit: bool,

    /// Make the underlying /spfs filesystem read-only (default)
    #[clap(long, overrides_with = "edit")]
    pub no_edit: bool,

    /// A name to use for the created spfs runtime (useful for rejoining it later)
    #[clap(long)]
    pub runtime_name: Option<String>,

    /// Keep the runtime around rather than deleting it when the
    /// process exits. This is best used with --runtime-name NAME to
    /// make the runtime easier to reuse later.
    #[clap(long, env = SPK_KEEP_RUNTIME)]
    pub keep_runtime: bool,

    /// Path to a live layer config file that will be added to the
    /// /spfs filesystem over the top of the existing spfs layers
    #[clap(long, value_name = "LIVE_LAYER_FILE")]
    pub live_layer: Option<Vec<String>>,
}

impl Runtime {
    /// True if the flags are requesting an editable runtime
    pub fn editable(&self) -> bool {
        // clap will ensure that edit is only true if provided
        // after --no-edit and because false is the default we
        // don't need to explicitly check no_edit
        self.edit
    }

    /// Unless `--no-runtime` is present, relaunch the current process inside
    /// a new spfs runtime.
    ///
    /// The caller is expected to pass in a list of subcommand aliases that can
    /// be used to find an appropriate place on the command line to insert a
    /// `--no-runtime` argument, to avoid recursively creating a runtime.
    pub async fn ensure_active_runtime(
        &self,
        sub_command_aliases: &[&str],
    ) -> Result<spfs::runtime::Runtime> {
        if self.no_runtime {
            return Ok(spfs::active_runtime().await?);
        }
        // Find where to insert a `--no-runtime` flag into the existing
        // command line.
        #[cfg(target_os = "linux")]
        {
            let no_runtime_arg_insertion_index = std::env::args()
                // Skip the first arg because it is the application name and
                // could be anything, including something that matches one of the
                // subcommand aliases given.
                .skip(1)
                .position(|arg| sub_command_aliases.iter().any(|command| arg == *command))
                // Add 2 to the index if we found it since the first element was
                // skipped and we're supposed to give the insertion index which
                // is after the position of the element we found.
                .map(|index| index + 2)
                // Default to 2 because that's the correct position on most cases.
                .unwrap_or(2);

            self.relaunch_with_runtime(no_runtime_arg_insertion_index)
        }

        #[cfg(target_os = "windows")]
        {
            // Prevent unused variable warning.
            let _ = sub_command_aliases.is_empty();

            unimplemented!()
        }
    }

    /// Relaunch the current process inside a new spfs runtime.
    ///
    /// To prevent the relaunched process from attempting to relaunch itself
    /// again recursively, the caller must be a process that accept the
    /// command line flag `--no-runtime`, and must specify what argument
    /// position is appropriate to insert this flag.
    ///
    /// For example:
    ///
    /// ["spk", "env", "pkg1", "pkg2"]
    ///  0      1      2       3
    ///
    /// `relaunch_with_runtime(2)` becomes:
    ///
    /// ["spfs", "run", "-", "--", "spk", "env", "--no-runtime", "pkg1", "pkg2"]
    #[cfg(target_os = "linux")]
    pub fn relaunch_with_runtime(
        &self,
        no_runtime_arg_insertion_index: usize,
    ) -> Result<spfs::runtime::Runtime> {
        use std::os::unix::ffi::OsStrExt;

        let args = std::env::args_os();

        let spfs = std::ffi::CString::new("spfs").expect("should never fail");
        let mut found_insertion_index = false;
        let mut args = args
            .enumerate()
            .flat_map(|(index, arg)| {
                if index == no_runtime_arg_insertion_index {
                    found_insertion_index = true;
                    vec![
                        std::ffi::CString::new("--no-runtime"),
                        std::ffi::CString::new(arg.as_bytes()),
                    ]
                } else {
                    vec![std::ffi::CString::new(arg.as_bytes())]
                }
            })
            .collect::<std::result::Result<Vec<_>, _>>()
            .into_diagnostic()
            .wrap_err("One or more arguments were not a valid c-string")?;
        if !found_insertion_index {
            args.push(std::ffi::CString::new("--no-runtime").expect("--no-runtime is valid UTF-8"));
        }
        args.insert(0, std::ffi::CString::new("--").expect("should never fail"));
        args.insert(
            0,
            std::ffi::CString::new(spfs::tracking::ENV_SPEC_EMPTY).expect("should never fail"),
        );
        if let Some(runtime_name) = &self.runtime_name {
            // Inject '--runtime-name <name>' so the runtime will be named
            args.insert(
                0,
                std::ffi::CString::new(runtime_name.clone()).expect("should never fail"),
            );
            args.insert(
                0,
                std::ffi::CString::new("--runtime-name").expect("should never fail"),
            );
        }

        if self.keep_runtime {
            args.insert(
                0,
                std::ffi::CString::new("--keep-runtime").expect("should never fail"),
            );
        }

        args.insert(0, std::ffi::CString::new("run").expect("should never fail"));
        args.insert(0, spfs.clone());

        tracing::debug!("relaunching under spfs");
        tracing::trace!("{:?}", args);

        // Record the run duration up to this point because this spk
        // command is about to replace itself with an identical spk
        // command that is inside a spfs runtime. We want to capture
        // the run time for the current spk run before it is replaced.
        #[cfg(feature = "statsd")]
        {
            if let Some(statsd_client) = get_metrics_client() {
                statsd_client.record_duration_from_start(&SPK_RUN_TIME_METRIC);
            }
        }

        nix::unistd::execvp(&spfs, args.as_slice())
            .into_diagnostic()
            .wrap_err("Failed to re-launch spk in an spfs runtime")?;
        unreachable!()
    }
}

#[derive(Args, Clone)]
pub struct Solver {
    #[clap(flatten)]
    pub repos: Repositories,

    /// If true, build packages from source if needed
    #[clap(long)]
    pub allow_builds: bool,

    /// If true, the solver will run impossible request checks on the initial requests
    #[clap(long, env = "SPK_SOLVER_CHECK_IMPOSSIBLE_INITIAL")]
    pub check_impossible_initial: bool,

    /// If true, the solver will run impossible request checks before
    /// using a package build to resolve a request
    #[clap(long, env = "SPK_SOLVER_CHECK_IMPOSSIBLE_VALIDATION")]
    pub check_impossible_validation: bool,

    /// If true, the solver will run impossible request checks to
    /// use in the build keys for ordering builds during the solve
    #[clap(long, env = "SPK_SOLVER_CHECK_IMPOSSIBLE_BUILDS")]
    pub check_impossible_builds: bool,

    /// If true, the solver will run all three impossible request checks: initial
    /// requests, build validation before a resolve, and for build keys
    #[clap(long, env = "SPK_SOLVER_CHECK_IMPOSSIBLE_ALL")]
    pub check_impossible_all: bool,
}

impl Solver {
    pub async fn get_solver(&self, options: &Options) -> Result<solve::StepSolver> {
        let option_map = options.get_options()?;

        let mut solver = solve::StepSolver::default();
        solver.update_options(option_map);

        for (name, repo) in self.repos.get_repos_for_non_destructive_operation().await? {
            tracing::debug!(repo=%name, "using repository");
            solver.add_repository(repo);
        }
        solver.set_binary_only(!self.allow_builds);
        solver.set_initial_request_impossible_checks(
            self.check_impossible_initial || self.check_impossible_all,
        );
        solver.set_resolve_validation_impossible_checks(
            self.check_impossible_validation || self.check_impossible_all,
        );
        solver.set_build_key_impossible_checks(
            self.check_impossible_builds || self.check_impossible_all,
        );

        for r in options.get_var_requests()? {
            solver.add_request(r.into());
        }

        Ok(solver)
    }
}

#[derive(Args, Clone)]
pub struct Options {
    /// Specify build/resolve options
    ///
    /// When building packages, these options are used to specify
    /// inputs to the build itself as well as parameters for resolving the
    /// build environment. In other cases, the options are used to
    /// limit which packages builds can be used/resolved.
    ///
    /// Options are specified as key/value pairs separated by either
    /// an equals sign or colon (--opt name=value --opt other:value).
    /// Additionally, many options can be specified at once in yaml
    /// or json format (--opt '{name: value, other: value}').
    ///
    /// Options can also be given in a file via the --options-file/-f flag. If
    /// given, --opt will supersede anything in the options file(s).
    #[clap(long = "opt", short)]
    pub options: Vec<String>,

    /// Do not add the default options for the current host system
    #[clap(long)]
    pub no_host: bool,
}

impl Options {
    pub fn get_options(&self) -> Result<OptionMap> {
        let mut opts = match self.no_host {
            true => OptionMap::default(),
            false => HOST_OPTIONS
                .get()
                .wrap_err("Failed to compute options for current host")?,
        };

        for pair in self.options.iter() {
            let pair = pair.trim();
            if pair.starts_with('{') {
                let given: OptionMap = serde_yaml::from_str(pair)
                    .into_diagnostic()
                    .wrap_err("--opt value looked like yaml, but could not be parsed")?;
                opts.extend(given);
                continue;
            }

            let (name, value) = pair
                .split_once('=')
                .or_else(|| pair.split_once(':'))
                .ok_or_else(|| {
                    miette!("Invalid option: -o {pair} (should be in the form name=value)")
                })
                .and_then(|(name, value)| Ok((OptName::new(name)?, value)))?;

            opts.insert(name.to_owned(), value.to_string());
        }

        Ok(opts)
    }

    pub fn get_var_requests(&self) -> Result<Vec<VarRequest>> {
        Ok(self
            .get_options()?
            .into_iter()
            .filter(|(_name, value)| !value.is_empty())
            .map(|(name, value)| VarRequest::new_with_value(name, value))
            .collect())
    }
}

#[derive(Args, Clone)]
pub struct Requests {
    /// Allow pre-releases for all command line package requests
    #[clap(long)]
    pub pre: bool,

    #[clap(flatten)]
    pub workspace: Workspace,
}

impl Requests {
    /// Resolve command line requests to package identifiers.
    pub async fn parse_idents<'a, I: IntoIterator<Item = &'a str>>(
        &self,
        options: &OptionMap,
        packages: I,
        repos: &[Arc<storage::RepositoryHandle>],
    ) -> Result<Vec<AnyIdent>> {
        let mut idents = Vec::new();
        let mut workspace = None;
        for package in packages {
            if package.contains('@') {
                if workspace.is_none() {
                    workspace = Some(self.workspace.load_or_default()?);
                }
                let Some(ws) = workspace.as_mut() else {
                    unreachable!();
                };

                let (recipe, _, stage, _) =
                    parse_stage_specifier(package, options, ws, repos).await?;

                match stage {
                    TestStage::Sources => {
                        let ident = recipe.ident().to_any_ident(Some(Build::Source));
                        idents.push(ident);
                        continue;
                    }
                    _ => {
                        bail!(
                            "Unsupported stage '{stage}', can only be empty or 'source' in this context"
                        );
                    }
                }
            }

            let path = std::path::Path::new(package);
            if path.is_file() {
                if workspace.is_none() {
                    workspace = Some(self.workspace.load_or_default()?);
                }
                let Some(ws) = workspace.as_mut() else {
                    unreachable!();
                };

                let configured = ws
                    .find_or_load_package_template(package)
                    .wrap_err("did not find recipe template")?;
                let rendered_data = configured.template.render(options)?;
                let recipe = rendered_data.into_recipe().wrap_err_with(|| {
                    format!(
                        "{filename} was expected to contain a recipe",
                        filename = configured.template.file_path().to_string_lossy()
                    )
                })?;
                idents.push(recipe.ident().to_any_ident(None));
            } else {
                idents.push(parse_ident(package)?)
            }
        }

        Ok(idents)
    }

    /// Parse and build a request, and any extra options, from the
    /// given string and these flags. If the request expands into
    /// multiple requests, such as from a request file, this will
    /// return the last request. Any options returned are filtered to
    /// exclude any (override) options given in the options parameter.
    pub async fn parse_request<R: AsRef<str>>(
        &self,
        request: R,
        options: &Options,
        repos: &[Arc<storage::RepositoryHandle>],
    ) -> Result<(Request, OptionMap)> {
        let (mut requests, extra_options) = self
            .parse_requests([request.as_ref()], options, repos)
            .await?;
        let last_request = requests.pop().unwrap();
        Ok((last_request, extra_options))
    }

    /// Parse and build requests, and any extra options, from the
    /// given strings and these flags. Any options returned are
    /// filtered to exclude any (override) options given in the
    /// options parameter.
    pub async fn parse_requests<I, S>(
        &self,
        requests: I,
        options: &Options,
        repos: &[Arc<storage::RepositoryHandle>],
    ) -> Result<(Vec<Request>, OptionMap)>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut out = Vec::<Request>::new();
        let override_options = options.get_options()?;
        let mut templating_options = override_options.clone();
        let mut extra_options = OptionMap::default();
        let mut workspace = None;

        // From the positional REQUESTS arg
        for r in requests.into_iter() {
            let r: &str = r.as_ref();

            // Is it a filepath to a package requests yaml file?
            if r.ends_with(".spk.yaml") {
                if workspace.is_none() {
                    workspace = Some(self.workspace.load_or_default()?);
                }
                let Some(ws) = workspace.as_mut() else {
                    unreachable!();
                };

                let (spec, filename) = find_package_recipe_from_workspace_or_repo(
                    Some(&r),
                    &templating_options,
                    ws,
                    repos,
                )
                .await
                .wrap_err_with(|| format!("finding requests file {r}"))?;
                let requests_from_file = spec.into_requests().wrap_err_with(|| {
                    format!(
                        "{filename} was expected to contain a list of requests",
                        filename = filename.to_string_lossy()
                    )
                })?;

                out.extend(requests_from_file.requirements);

                for (name, value) in requests_from_file.options {
                    // Command line override options take precedence.
                    // Only when there is no command line override for
                    // this option name is it used
                    if override_options.get(&name).is_none() {
                        // For template values in later files and specs
                        templating_options.insert(OptName::new(&name)?.into(), value.clone());
                        // For later use by commands, usually when
                        // setting up a solver
                        extra_options.insert(OptName::new(&name)?.into(), value);
                    }
                }
                continue;
            }

            let reqs = self
                .parse_cli_or_pkg_file_request(r, &templating_options, &mut workspace, repos)
                .await?;
            out.extend(reqs);
        }

        if out.is_empty() {
            Err(Error::String(
                "Needs at least one request: Missing required argument <REQUESTS> ... ".to_string(),
            )
            .into())
        } else {
            Ok((out, extra_options))
        }
    }

    async fn parse_cli_or_pkg_file_request(
        &self,
        request: &str,
        options: &OptionMap,
        workspace: &mut Option<spk_workspace::Workspace>,
        repos: &[Arc<storage::RepositoryHandle>],
    ) -> Result<Vec<Request>> {
        // Parses a command line request into one or more requests.
        // 'file@stage' strings can expand into more than one request.
        let mut out = Vec::<Request>::new();

        if request.contains('@') {
            if workspace.is_none() {
                *workspace = Some(self.workspace.load_or_default()?);
            }
            let Some(ws) = workspace.as_mut() else {
                unreachable!();
            };

            let (recipe, _, stage, build_variant) =
                parse_stage_specifier(request, options, ws, repos)
                    .await
                    .wrap_err_with(|| {
                        format!("parsing {request} as a filename with stage specifier")
                    })?;

            match stage {
                TestStage::Sources => {
                    if build_variant.is_some() {
                        bail!("Source stage does not accept a build variant specifier")
                    }

                    let ident = recipe.ident().to_any_ident(Some(Build::Source));
                    out.push(PkgRequest::from_ident_exact(ident, RequestedBy::CommandLine).into());
                }

                TestStage::Build => {
                    let requirements = match build_variant {
                        Some(VariantIndex(index)) => {
                            let default_variants = recipe.default_variants(options);
                            let variant =
                                    default_variants
                                        .iter()
                                        .skip(index)
                                        .take(1)
                                        .next()
                                        .ok_or_else(|| miette!(
                                            "Variant {index} is out of range; {} variants(s) found in {}",
                                            default_variants.len(),
                                            recipe.ident().format_ident()
                                        ))?
                                        .with_overrides(options.clone());
                            recipe.get_build_requirements(&variant)?
                        }
                        None => recipe.get_build_requirements(&options)?,
                    };
                    out.extend(requirements.into_owned());
                }

                TestStage::Install => {
                    if build_variant.is_some() {
                        bail!("Install stage does not accept a build variant specifier")
                    }

                    out.push(
                        PkgRequest::from_ident_exact(
                            recipe.ident().to_any_ident(None),
                            RequestedBy::CommandLine,
                        )
                        .into(),
                    )
                }
            }
            return Ok(out);
        }

        // This is request without a '@' stage specifier
        let value: serde_yaml::Value = serde_yaml::from_str(request)
            .into_diagnostic()
            .wrap_err("Request was not a valid yaml value")?;
        let mut request_data = match value {
            v @ serde_yaml::Value::String(_) => {
                let mut mapping = serde_yaml::Mapping::with_capacity(1);
                mapping.insert("pkg".into(), v);
                mapping
            }
            serde_yaml::Value::Mapping(m) => m,
            _ => {
                bail!(
                    "Invalid request, expected either a string or a mapping, got: {:?}",
                    value
                )
            }
        };

        let prerelease_policy_key = "prereleasePolicy".into();
        if self.pre && !request_data.contains_key(&prerelease_policy_key) {
            request_data.insert(prerelease_policy_key, "IncludeAll".into());
        }

        let mut req = serde_yaml::from_value::<Request>(request_data.into())
            .into_diagnostic()
            .wrap_err_with(|| format!("Failed to parse request {request}"))?
            .pkg()
            .ok_or_else(|| miette!("Expected a package request, got None"))?;
        req.add_requester(RequestedBy::CommandLine);

        if req.pkg.components.is_empty() {
            if req.pkg.is_source() {
                req.pkg.components.insert(Component::Source);
            } else {
                req.pkg.components.insert(Component::default_for_run());
            }
        }
        if req.required_compat.is_none() {
            req.required_compat = Some(CompatRule::API);
        }
        out.push(req.into());

        Ok(out)
    }

    /// Returns Ok(true) if the requests contain any request with a
    /// @build stage specified, otherwise it returns Ok(false).
    pub fn any_build_stage_requests<I, S>(&self, requests: I) -> Result<bool>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        for r in requests.into_iter() {
            let r = r.as_ref();
            if r.contains('@') {
                let (_, stage, _) = parse_package_stage_and_variant(r)?;
                if stage == TestStage::Build {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }
}

/// Returns the package, stage, and build variant for the given specifier
fn parse_package_stage_and_variant(
    specifier: &str,
) -> Result<(&str, TestStage, Option<crate::parsing::VariantIndex>)> {
    use nom::combinator::all_consuming;

    let (package, stage, build_variant) =
        all_consuming::<_, _, nom_supreme::error::ErrorTree<_>, _>(stage_specifier)(specifier)
            .map(|(_, (package, stage, build_variant))| (package, stage, build_variant))
            .map_err(|err| match err {
                nom::Err::Error(e) | nom::Err::Failure(e) => Error::String(e.to_string()),
                nom::Err::Incomplete(_) => unreachable!(),
            })?;

    Ok((package, stage, build_variant))
}

/// Returns the spec, filename, stage, and build variant for the given specifier
pub async fn parse_stage_specifier(
    specifier: &str,
    options: &OptionMap,
    workspace: &mut spk_workspace::Workspace,
    repos: &[Arc<storage::RepositoryHandle>],
) -> Result<(
    Arc<SpecRecipe>,
    std::path::PathBuf,
    TestStage,
    Option<crate::parsing::VariantIndex>,
)> {
    let (package, stage, build_variant) = parse_package_stage_and_variant(specifier)
        .wrap_err_with(|| format!("parsing {specifier} as a stage name and optional variant"))?;
    let (spec, filename) =
        find_package_recipe_from_workspace_or_repo(Some(&package), options, workspace, repos)
            .await
            .wrap_err_with(|| format!("finding package recipe for {package}"))?;

    let recipe = spec.into_recipe().wrap_err_with(|| {
        format!(
            "{filename} was expected to contain a recipe",
            filename = filename.to_string_lossy()
        )
    })?;

    Ok((recipe, filename, stage, build_variant))
}

#[derive(Args, Default, Clone)]
pub struct Workspace {
    /// The location of the spk workspace to find spec files in
    #[clap(long, default_value = ".")]
    pub workspace: std::path::PathBuf,
}

impl Workspace {
    pub fn load_or_default(&self) -> Result<spk_workspace::Workspace> {
        match spk_workspace::Workspace::builder().load_from_dir(&self.workspace) {
            Ok(w) => {
                tracing::debug!(workspace = ?self.workspace, "Loading workspace");
                w.build().into_diagnostic()
            }
            Err(spk_workspace::error::FromPathError::LoadWorkspaceFileError(
                spk_workspace::error::LoadWorkspaceFileError::NoWorkspaceFile(_),
            ))
            | Err(spk_workspace::error::FromPathError::LoadWorkspaceFileError(
                spk_workspace::error::LoadWorkspaceFileError::WorkspaceNotFound(_),
            )) => {
                let mut builder = spk_workspace::Workspace::builder();

                if self.workspace.is_dir() {
                    tracing::debug!(
                        "Using virtual workspace in {d}",
                        d = self.workspace.to_string_lossy()
                    );
                    builder = builder.with_root(&self.workspace);
                } else {
                    tracing::debug!("Using virtual workspace in current dir");
                }

                builder
                    .with_glob_pattern("*.spk.yaml")?
                    .build()
                    .into_diagnostic()
                    .wrap_err("loading *.spk.yaml")
            }
            Err(err) => Err(err.into()),
        }
    }
}

/// Specifies a package, allowing for more details when being invoked
/// programmatically instead of by a user on the command line.
#[derive(Clone, Debug)]
pub enum PackageSpecifier {
    Plain(String),
    WithSourceIdent((String, RangeIdent)),
}

impl PackageSpecifier {
    // Return the package spec or filename string.
    pub fn get_specifier(&self) -> &String {
        match self {
            PackageSpecifier::Plain(s) => s,
            PackageSpecifier::WithSourceIdent((s, _)) => s,
        }
    }

    // Extract the package spec or filename string.
    pub fn into_specifier(self) -> String {
        match self {
            PackageSpecifier::Plain(s) => s,
            PackageSpecifier::WithSourceIdent((s, _)) => s,
        }
    }
}

impl std::str::FromStr for PackageSpecifier {
    type Err = clap::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // On the command line, only `Plain` is possible.
        Ok(PackageSpecifier::Plain(s.to_owned()))
    }
}

#[derive(Args, Default, Clone)]
pub struct Packages {
    /// The package names or yaml spec files to operate on
    ///
    /// Package requests may also come with a version when multiple
    /// versions might be found in the local workspace or configured
    /// repositories.
    #[clap(name = "PKG|SPEC_FILE")]
    pub packages: Vec<PackageSpecifier>,

    #[clap(flatten)]
    pub workspace: Workspace,
}

impl CommandArgs for Packages {
    fn get_positional_args(&self) -> Vec<String> {
        self.packages
            .iter()
            .map(|ps| ps.get_specifier())
            .cloned()
            .collect()
    }
}

impl Packages {
    /// Create clones of these arguments where each instance
    /// has only one package specified.
    ///
    /// Useful for running multiple package operations for each
    /// entry in order.
    pub fn split(&self) -> Vec<Self> {
        self.packages
            .iter()
            .cloned()
            .map(|p| Self {
                packages: vec![p],
                ..self.clone()
            })
            .collect()
    }

    pub async fn find_all_recipes(
        &self,
        options: &OptionMap,
        repos: &[Arc<storage::RepositoryHandle>],
    ) -> Result<Vec<(Option<PackageSpecifier>, SpecFileData, std::path::PathBuf)>> {
        let mut packages: Vec<_> = self.packages.iter().cloned().map(Some).collect();
        if packages.is_empty() {
            packages.push(None)
        }

        let mut workspace = self.workspace.load_or_default()?;

        let mut results = Vec::with_capacity(packages.len());
        for package in packages {
            let (file_data, path) = find_package_recipe_from_workspace_or_repo(
                package.as_ref().map(|p| p.get_specifier()),
                options,
                &mut workspace,
                repos,
            )
            .await?;
            results.push((package, file_data, path));
        }
        Ok(results)
    }
}

// TODO: rename this because it is more than package recipe spec now?
/// Find a package recipe either from a template file in the current
/// directory, or published version of the requested package, if any.
///
/// This function tries to discover the matching yaml template file
/// and populate it using the options. If it cannot find a file, it
/// will try to find the matching package/version in the repo and use
/// the recipe published for that.
///
pub async fn find_package_recipe_from_workspace_or_repo<S>(
    package_name: Option<&S>,
    options: &OptionMap,
    workspace: &mut spk_workspace::Workspace,
    repos: &[Arc<storage::RepositoryHandle>],
) -> Result<(SpecFileData, std::path::PathBuf)>
where
    S: AsRef<str>,
{
    let from_workspace = match package_name {
        Some(package_name) => workspace.find_or_load_package_template(package_name),
        None => workspace.default_package_template().map_err(From::from),
    };
    let configured = match from_workspace {
        Ok(template) => template,
        res @ Err(FindOrLoadPackageTemplateError::FindPackageTemplateError(
            FindPackageTemplateError::MultipleTemplates(_),
        ))
        | res @ Err(FindOrLoadPackageTemplateError::BuildError(_)) => {
            res.wrap_err("did not find recipe template")?
        }
        res @ Err(FindOrLoadPackageTemplateError::FindPackageTemplateError(
            FindPackageTemplateError::NoTemplateFiles,
        ))
        | res @ Err(FindOrLoadPackageTemplateError::FindPackageTemplateError(
            FindPackageTemplateError::NotFound(..),
        )) => {
            drop(res); // promise that we don't hold data from the workspace anymore

            // If couldn't find a template file, maybe there's an
            // existing package/version that's been published
            match package_name.map(AsRef::as_ref) {
                Some(name) if std::path::Path::new(name).is_file() => {
                    tracing::debug!(?name, "Loading anonymous template file into workspace...");
                    workspace.load_template_file(name)?
                }
                Some(name) => {
                    tracing::debug!("Unable to find package file: {}", name);
                    // there will be at least one item for any string
                    let name_version = name.split('@').next().unwrap();

                    // If the package name can't be parsed as a valid name,
                    // don't return the parse error. It's possible that the
                    // name is a filename like "package.spk.yaml" which is an
                    // illegal package name and won't parse successfully. Let
                    // this get reported as missing file below.
                    if let Ok(pkg) = parse_ident(name_version) {
                        tracing::debug!(
                            "Looking in repositories for a package matching {} ...",
                            pkg.format_ident()
                        );

                        for repo in repos.iter() {
                            match repo.read_recipe(pkg.as_version_ident()).await {
                                Ok(recipe) => {
                                    tracing::debug!(
                                        "Using recipe found for {}",
                                        recipe.ident().format_ident(),
                                    );
                                    return Ok((
                                        SpecFileData::Recipe(recipe),
                                        std::path::PathBuf::from(name),
                                    ));
                                }

                                Err(spk_storage::Error::PackageNotFound(_)) => continue,
                                Err(err) => return Err(err.into()),
                            }
                        }
                    }

                    miette::bail!(
                        help = "Check that file path, or package/version request, is correct",
                        "Unable to find {name:?} as a file, or existing package/version recipe in any repo",
                    );
                }
                None => {
                    miette::bail!(
                        help = "Provide a file path, or package/version request",
                        "Unable to find a spec file, or existing package/version"
                    );
                }
            }
        }
    };
    let found = configured.template.render(options).wrap_err_with(|| {
        format!(
            "{filename} was expected to contain a valid spk yaml data file",
            filename = configured.template.file_path().to_string_lossy()
        )
    })?;
    tracing::debug!(
        "Rendered configured.template from the data in {:?}",
        configured.template.file_path()
    );
    Ok((found, configured.template.file_path().to_owned()))
}

#[derive(Args, Clone)]
pub struct Repositories {
    /// This option will enable the local repository only.
    /// Use `--no-local-repo` to disable the local repository.
    #[clap(short = 'L', long)]
    pub local_repo_only: bool,

    /// Disable the local repository
    #[clap(long, hide = true)]
    pub no_local_repo: bool,

    /// Repositories to enable for the command
    ///
    /// Any configured spfs repository can be named here as well as "local" or
    /// a path on disk or a full remote repository url. Repositories can also
    /// be limited to a specific time by appending a relative or absolute time
    /// specifier (eg: origin~10m, origin~5weeks, origin@2022-10-11,
    /// origin@2022-10-11T13:00.12). This time affects all interactions and
    /// queries in the repository, effectively making it look like it did in the past.
    /// It will cause errors for any operation that attempts to make changes to
    /// the repository, even if the time is in the future.
    #[clap(long, short = 'r')]
    pub enable_repo: Vec<String>,

    /// Repositories to exclude from the command
    ///
    /// Any configured spfs repository can be named here as well as "local"
    #[clap(long)]
    pub disable_repo: Vec<String>,

    /// Limit all repository data to a point in time
    ///
    /// A relative or absolute time to apply to all local and remote repositories
    /// (eg: ~10m, ~5weeks, @2022-10-11, @2022-10-11T13:00.12). This value is superseded
    /// at an individual level by any time specifier added to the --enable-repo/-r flag.
    ///
    /// This time affects all interactions and queries in the repository, effectively making
    /// it look like spk is being run in the past. It will cause errors for any operation
    /// that attempts to make changes to a repository, even if the time is in the future.
    #[clap(long)]
    pub when: Option<spfs::tracking::TimeSpec>,

    /// Enable support for legacy spk version tags in the repository.
    ///
    /// This causes extra file I/O but is required if the repository contains
    /// any packages that were published with non-normalized version tags.
    ///
    /// This is enabled by default if spk is built with the legacy-spk-version-tags
    /// feature flag enabled.
    #[clap(long, hide = true)]
    pub legacy_spk_version_tags: bool,
}

impl Repositories {
    /// Get the repositories to use based on command-line options.
    ///
    /// This method enables the local repository by default, except if any
    /// repositories have been enabled with `--enable-repo`, or if
    /// `--no-local-repo` is used.
    pub async fn get_repos_for_destructive_operation(
        &self,
    ) -> Result<Vec<(String, storage::RepositoryHandle)>> {
        let mut enabled = Vec::with_capacity(self.enable_repo.len());
        let disabled: HashSet<&str> = self.disable_repo.iter().map(String::as_str).collect();
        for r in self.enable_repo.iter() {
            match r.find(['~', '@']) {
                Some(i) => enabled.push((&r[..i], Some(spfs::tracking::TimeSpec::parse(&r[i..])?))),
                None => enabled.push((r, None)),
            };
        }

        let mut repos = Vec::with_capacity(enabled.len());
        if !self.no_local_repo
            && self.enable_repo.is_empty()
            // Interpret `--disable-repo local` as a request to not use the
            // local repo.
            && !disabled.contains("local")
        {
            let mut repo = storage::local_repository().await?;
            if let Some(ts) = self.when.as_ref() {
                repo.pin_at_time(ts);
            }
            if self.legacy_spk_version_tags {
                repo.set_legacy_spk_version_tags(true);
            }
            repos.push(("local".into(), repo.into()));
        }
        for (name, ts) in enabled.iter() {
            if disabled.contains(name) {
                continue;
            }

            if let Some(i) = repos.iter().position(|(n, _)| n == name) {
                // we favor the last instance of an --enable-repo flag
                // over any previous one in the case of duplicates
                repos.remove(i);
            }

            let mut repo = match *name {
                // Allow `--enable-repo local` to work to enable the local repo.
                "local" => storage::local_repository().await,
                name => storage::remote_repository(name).await,
            }?;
            if let Some(ts) = ts.as_ref().or(self.when.as_ref()) {
                repo.pin_at_time(ts);
            }
            if self.legacy_spk_version_tags {
                repo.set_legacy_spk_version_tags(true);
            }
            repos.push((name.to_string(), repo.into()));
        }
        Ok(repos.into_iter().collect())
    }

    /// Get the repositories to use based on command-line options.
    ///
    /// This method enables the "local" and "origin" repositories by default.
    /// This behavior can be altered with the `--enable-repo`, `--disable-repo`,
    /// and `--no-local-repo` flags.
    ///
    /// The `--enable-repo` is considered additive instead of exclusive.
    ///
    /// Remote repos enabled with `--enable-repo` are added to the list before
    /// "origin".
    pub async fn get_repos_for_non_destructive_operation(
        &self,
    ) -> Result<Vec<(String, storage::RepositoryHandle)>> {
        let mut enabled = Vec::with_capacity(self.enable_repo.len());
        let disabled: HashSet<&str> = self.disable_repo.iter().map(String::as_str).collect();
        for r in self.enable_repo.iter() {
            match r.find(['~', '@']) {
                Some(i) => enabled.push((&r[..i], Some(spfs::tracking::TimeSpec::parse(&r[i..])?))),
                None => enabled.push((r, None)),
            };
        }

        let mut repos = Vec::new();
        if !self.no_local_repo
            // Interpret `--disable-repo local` as a request to not use the
            // local repo.
            && !disabled.contains("local")
        {
            let mut repo = storage::local_repository().await?;
            if let Some(ts) = self.when.as_ref() {
                repo.pin_at_time(ts);
            }
            if self.legacy_spk_version_tags {
                repo.set_legacy_spk_version_tags(true);
            }
            repos.push(("local".into(), repo.into()));
        }
        if self.local_repo_only {
            return Ok(repos);
        }
        for (name, ts, is_default_origin) in enabled
            .into_iter()
            .map(|(name, ts)| (name, ts, false))
            .chain([("origin", None, true)])
        {
            if disabled.contains(name) {
                continue;
            }
            if let Some(i) = repos.iter().position(|(n, _)| n == name) {
                // We favor the last instance of an --enable-repo flag
                // over any previous one in the case of duplicates, except
                // any explicit "origin" overrides the default.
                if is_default_origin {
                    // Keep the explicitly enabled "origin" repo.
                    continue;
                }
                repos.remove(i);
            }

            let mut repo = match name {
                // Allow `--enable-repo local` to work to enable the local repo.
                "local" => storage::local_repository().await,
                name => match storage::remote_repository(name).await {
                    Err(spk_storage::Error::SPFS(spfs::Error::UnknownRemoteName(_)))
                        if is_default_origin =>
                    {
                        // "origin" is not required to exist when attempting to
                        // add it as a default
                        continue;
                    }
                    other => other,
                },
            }?;
            if let Some(ts) = ts.as_ref().or(self.when.as_ref()) {
                repo.pin_at_time(ts);
            }
            if self.legacy_spk_version_tags {
                repo.set_legacy_spk_version_tags(true);
            }
            repos.push((name.into(), repo.into()));
        }
        Ok(repos)
    }
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub enum SolverToRun {
    /// Run and show output from the basic solver
    Cli,
    /// Run and show output from the "impossible requests" checking solver
    Checks,
    /// Run both solvers, showing the output from the basic solver,
    /// unless overridden with --solver-to-run
    All,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub enum SolverToShow {
    /// Show output from the basic solver
    Cli,
    /// Show output from the "impossible requests" checking solver
    Checks,
}

impl From<SolverToRun> for MultiSolverKind {
    fn from(item: SolverToRun) -> MultiSolverKind {
        match item {
            SolverToRun::Cli => MultiSolverKind::Unchanged,
            SolverToRun::Checks => MultiSolverKind::AllImpossibleChecks,
            SolverToRun::All => MultiSolverKind::All,
        }
    }
}

impl From<SolverToShow> for MultiSolverKind {
    fn from(item: SolverToShow) -> MultiSolverKind {
        match item {
            SolverToShow::Cli => MultiSolverKind::Unchanged,
            SolverToShow::Checks => MultiSolverKind::AllImpossibleChecks,
        }
    }
}

#[derive(Args, Clone)]
pub struct DecisionFormatterSettings {
    /// If true, display solver time and stats after each solve
    #[clap(short = 't', long)]
    pub time: bool,

    /// Increase the solver's verbosity every time this many seconds pass
    ///
    /// A solve has taken too long if it runs for more than this
    /// number of seconds and hasn't found a solution. Setting this
    /// above zero will increase the verbosity every that many seconds
    /// the solve runs. If this is zero, the solver's verbosity will
    /// not increase during a solve.
    #[clap(long, env = "SPK_SOLVER_TOO_LONG_SECONDS", default_value_t = 30)]
    pub increase_verbosity: u64,

    /// The maximum verbosity that automatic verbosity increases will
    /// stop at and not go above.
    ///
    #[clap(long, env = "SPK_SOLVER_VERBOSITY_INCREASE_LIMIT", default_value_t = 2)]
    pub max_verbosity_increase_level: u8,

    /// Maximum number of seconds to let the solver run before halting the solve
    ///
    /// Maximum number of seconds to allow a solver to run before
    /// halting the solve. If this is zero, which is the default, the
    /// timeout is disabled and the solver will run to completion.
    #[clap(long, env = "SPK_SOLVER_SOLVE_TIMEOUT", default_value_t = 0)]
    pub timeout: u64,

    /// Show the package builds in the solution for any solver
    /// run. This will be automatically enabled for 'build',
    /// 'make-binary', and 'explain' commands or if v > 0.
    #[clap(long)]
    pub show_solution: bool,

    /// Set the threshold of a longer than acceptable solves, in seconds.
    ///
    #[clap(long, env = "SPK_SOLVER_LONG_SOLVE_THRESHOLD", default_value_t = 15)]
    pub long_solves: u64,

    /// Set the limit for how many of the most frequent errors are
    /// displayed in solve stats reports
    #[clap(long, env = "SPK_SOLVER_MAX_FREQUENT_ERRORS", default_value_t = 15)]
    pub max_frequent_errors: usize,

    /// Display a visualization of the solver progress if the solve takes longer
    /// than a few seconds.
    #[clap(long)]
    pub status_bar: bool,

    /// Control which solver(s) are run. The default is to run all the
    /// solvers in parallel and show the 'cli' solver's output. See
    /// also --solver-to-show.
    ///
    /// There are currently two modes for the solver, one that is faster when
    /// there are few problems encountered looking for packages (cli) and one
    /// that is faster when it is difficult to find a set of packages that
    /// satisfy a request (checks).
    ///
    /// By default, both solvers are run in parallel and the result is
    /// taken from the first one that finishes, and the output from
    /// the (cli) solver is displayed. even if the result ultimately
    /// comes from the (checks) solver. To run only one solver, use
    /// `--solver-to-run <cli|checks>`.
    #[clap(long, env = "SPK_SOLVER__SOLVER_TO_RUN", value_enum, default_value_t = SolverToRun::All)]
    pub solver_to_run: SolverToRun,
    /// Control which solver's output is shown when multiple solvers
    /// (all) are run. See also --solver-to-run.
    #[clap(long, env = "SPK_SOLVER__SOLVER_TO_SHOW", value_enum, default_value_t = SolverToShow::Cli)]
    pub solver_to_show: SolverToShow,

    /// Display a report on of the search space size for the resolved solution.
    #[clap(long)]
    pub show_search_size: bool,

    /// Run all the solvers to completion and produce a report
    /// comparing them.
    #[clap(long)]
    compare_solvers: bool,

    /// Stop the solver the first time it is BLOCKED.
    #[clap(long, alias = "stop")]
    stop_on_block: bool,

    /// Pause the solver each time it is blocked, until the user hits Enter.
    #[clap(long, alias = "step")]
    step_on_block: bool,

    /// Pause the solver each time it makes a decision, until the user hits Enter.
    #[clap(long, alias = "decision")]
    step_on_decision: bool,

    /// Capture each solver's output to a separate file in the given
    /// directory when a solver is run. The files will be named
    /// `<solver_file_prefix>_YYYYmmdd_HHMMSS_nnnnnnnn_<solver_kind>`. See
    /// --output-file-prefix for the default prefix and how to
    /// override it.
    #[clap(long, env = SPK_SOLVER_OUTPUT_TO_DIR, value_hint = ValueHint::FilePath)]
    output_to_dir: Option<std::path::PathBuf>,

    /// Set the minimum verbosity for solvers when outputting to a
    /// file. Has no effect unless --output-to-file is also specified.
    /// Verbosity set (-v) higher than this minimum will override it.
    #[clap(long, default_value_t=2, env = SPK_SOLVER_OUTPUT_TO_DIR_MIN_VERBOSITY)]
    output_to_dir_min_verbosity: u8,

    /// Override the default solver output filename prefix. The
    /// current date, time, and solver kind name will be appended to
    /// this prefix to produce the file name for each solver. See
    /// also --output-to-dir.
    #[clap(long, default_value_t=String::from(DEFAULT_SOLVER_RUN_FILE_PREFIX), env = SPK_SOLVER_OUTPUT_FILE_PREFIX)]
    output_file_prefix: String,
}

impl DecisionFormatterSettings {
    /// Get a decision formatter configured from the command line
    /// options and their defaults.
    pub fn get_formatter(&self, verbosity: u8) -> Result<DecisionFormatter> {
        Ok(self.get_formatter_builder(verbosity)?.build())
    }

    /// Get a decision formatter builder configured from the command
    /// line options and defaults and ready to call build() on, in
    /// case some extra configuration might be needed before calling
    /// build.
    pub fn get_formatter_builder(&self, verbosity: u8) -> Result<DecisionFormatterBuilder> {
        let mut builder =
            DecisionFormatterBuilder::try_from_config().wrap_err("Failed to load config")?;
        builder
            .with_verbosity(verbosity)
            .with_time_and_stats(self.time)
            .with_verbosity_increase_every({
                // If using the status bar, don't automatically increase
                // verbosity. The extra verbosity decreases the solver speed
                // significantly.
                if self.status_bar {
                    0
                } else {
                    self.increase_verbosity
                }
            })
            .with_max_verbosity_increase_level(self.max_verbosity_increase_level)
            .with_timeout(self.timeout)
            .with_solution(self.show_solution)
            .with_long_solves_threshold(self.long_solves)
            .with_max_frequent_errors(self.max_frequent_errors)
            .with_status_bar(self.status_bar)
            .with_solver_to_run(self.solver_to_run.into())
            .with_solver_to_show(self.solver_to_show.into())
            .with_search_space_size(self.show_search_size)
            .with_stop_on_block(self.stop_on_block)
            .with_step_on_block(self.step_on_block)
            .with_step_on_decision(self.step_on_decision)
            .with_output_to_dir(self.output_to_dir.clone())
            .with_output_to_dir_min_verbosity(self.output_to_dir_min_verbosity)
            .with_output_file_prefix(self.output_file_prefix.clone())
            .with_compare_solvers(self.compare_solvers);
        Ok(builder)
    }
}
