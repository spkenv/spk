// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

mod variant;

use std::collections::HashSet;
use std::convert::From;
use std::sync::Arc;

use clap::{Args, ValueEnum, ValueHint};
use miette::{bail, miette, Context, IntoDiagnostic, Result};
use solve::{
    DecisionFormatter,
    DecisionFormatterBuilder,
    MultiSolverKind,
    DEFAULT_SOLVER_RUN_FILE_PREFIX,
};
use spfs::runtime::LiveLayerFile;
use spk_schema::foundation::format::FormatIdent;
use spk_schema::foundation::ident_build::Build;
use spk_schema::foundation::ident_component::Component;
use spk_schema::foundation::name::OptName;
use spk_schema::foundation::option_map::OptionMap;
use spk_schema::foundation::spec_ops::Named;
use spk_schema::foundation::version::CompatRule;
use spk_schema::ident::{parse_ident, AnyIdent, PkgRequest, Request, RequestedBy, VarRequest};
use spk_schema::option_map::HOST_OPTIONS;
use spk_schema::{Recipe, SpecRecipe, SpecTemplate, Template, TemplateExt, TestStage, VariantExt};
#[cfg(feature = "statsd")]
use spk_solve::{get_metrics_client, SPK_RUN_TIME_METRIC};
pub use variant::{Variant, VariantBuildStatus, VariantLocation};
use {spk_solve as solve, spk_storage as storage};

use crate::parsing::{stage_specifier, VariantIndex};
use crate::Error;

#[cfg(test)]
#[path = "./flags_test.rs"]
mod flags_test;

static SPK_NO_RUNTIME: &str = "SPK_NO_RUNTIME";
static SPK_KEEP_RUNTIME: &str = "SPK_KEEP_RUNTIME";
static SPK_OUTPUT_TO_DIR: &str = "SPK_OUTPUT_TO_DIR";
static SPK_OUTPUT_TO_DIR_MIN_VERBOSITY: &str = "SPK_OUTPUT_TO_DIR_MIN_VERBOSITY";
static SPK_OUTPUT_FILE_PREFIX: &str = "SPK_OUTPUT_FILE_PREFIX";

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
    #[clap(long, value_name = "LAYER_FILE")]
    pub live_layer: Option<Vec<LiveLayerFile>>,
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
                std::ffi::CString::new("--name").expect("should never fail"),
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
    pub async fn get_solver(&self, options: &Options) -> Result<solve::Solver> {
        let option_map = options.get_options()?;
        let mut solver = solve::Solver::default();
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

    /// Specify build/resolve options from a json or yaml file (see --opt/-o)
    #[clap(long, value_hint = ValueHint::FilePath)]
    pub options_file: Vec<std::path::PathBuf>,

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

        for filename in self.options_file.iter() {
            let reader = std::fs::File::open(filename)
                .into_diagnostic()
                .wrap_err_with(|| format!("Failed to open: {filename:?}"))?;
            let options: OptionMap = serde_yaml::from_reader(reader)
                .into_diagnostic()
                .wrap_err_with(|| format!("Failed to parse as option mapping: {filename:?}"))?;
            opts.extend(options);
        }

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
        for package in packages {
            if package.contains('@') {
                let (recipe, _, stage, _) = parse_stage_specifier(package, options, repos).await?;

                match stage {
                    TestStage::Sources => {
                        let ident = recipe.ident().to_any(Some(Build::Source));
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
                let (_, template) = find_package_template(Some(&package))?.must_be_found();
                let recipe = template.render(options)?;
                idents.push(recipe.ident().to_any(None));
            } else {
                idents.push(parse_ident(package)?)
            }
        }

        Ok(idents)
    }

    /// Parse and build a request from the given string and these flags
    pub async fn parse_request<R: AsRef<str>>(
        &self,
        request: R,
        options: &Options,
        repos: &[Arc<storage::RepositoryHandle>],
    ) -> Result<Request> {
        Ok(self
            .parse_requests([request.as_ref()], options, repos)
            .await?
            .pop()
            .unwrap())
    }

    /// Parse and build requests from the given strings and these flags.
    pub async fn parse_requests<I, S>(
        &self,
        requests: I,
        options: &Options,
        repos: &[Arc<storage::RepositoryHandle>],
    ) -> Result<Vec<Request>>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut out = Vec::<Request>::new();
        let options = options.get_options()?;
        for r in requests.into_iter() {
            let r = r.as_ref();
            if r.contains('@') {
                let (recipe, _, stage, build_variant) = parse_stage_specifier(r, &options, repos)
                    .await
                    .wrap_err_with(|| format!("parsing {r} as a filename with stage specifier"))?;

                match stage {
                    TestStage::Sources => {
                        if build_variant.is_some() {
                            bail!("Source stage does not accept a build variant specifier")
                        }

                        let ident = recipe.ident().to_any(Some(Build::Source));
                        out.push(
                            PkgRequest::from_ident_exact(ident, RequestedBy::CommandLine).into(),
                        );
                    }

                    TestStage::Build => {
                        let requirements = match build_variant {
                            Some(VariantIndex(index)) => {
                                let default_variants = recipe.default_variants(&options);
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
                                recipe.ident().to_any(None),
                                RequestedBy::CommandLine,
                            )
                            .into(),
                        )
                    }
                }
                continue;
            }
            let value: serde_yaml::Value = serde_yaml::from_str(r)
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
                .wrap_err(format!("Failed to parse request {r}"))?
                .into_pkg()
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
        }

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
        find_package_recipe_from_template_or_repo(Some(&package), options, repos)
            .await
            .wrap_err_with(|| format!("finding package recipe for {package}"))?;

    Ok((spec, filename, stage, build_variant))
}

/// The result of the [`find_package_template`] function.
// We are okay with the large variant here because it's specifically
// used as the positive result of the function, with the others simply
// denoting unique error cases.
#[allow(clippy::large_enum_variant)]
pub enum FindPackageTemplateResult {
    /// A non-ambiguous package template file was found
    Found {
        path: std::path::PathBuf,
        template: Arc<SpecTemplate>,
    },
    /// No package was specifically requested, and there are multiple
    /// files in the current repository.
    MultipleTemplateFiles(Vec<std::path::PathBuf>),
    /// No package was specifically requested, and there no template
    /// files in the current repository.
    NoTemplateFiles,
    NotFound(String),
}

impl FindPackageTemplateResult {
    pub fn is_found(&self) -> bool {
        matches!(self, Self::Found { .. })
    }

    /// Prints error messages and exits if no template file was found
    pub fn must_be_found(self) -> (std::path::PathBuf, Arc<SpecTemplate>) {
        match self {
            Self::Found { path, template } => return (path, template),
            Self::MultipleTemplateFiles(files) => {
                tracing::error!("Multiple package specs in current directory:");
                for file in files {
                    tracing::error!("- {}", file.into_os_string().to_string_lossy());
                }
                tracing::error!(" > please specify a package name or filepath");
            }
            Self::NoTemplateFiles => {
                tracing::error!("No package specs found in current directory");
                tracing::error!(" > please specify a filepath");
            }
            Self::NotFound(request) => {
                tracing::error!("Spec file not found for '{request}', or the file does not exist");
            }
        }
        std::process::exit(1);
    }
}

/// Find a package template file for the requested package, if any.
///
/// This function will use the current directory and the provided
/// package name or filename to try and discover the matching
/// yaml template file.
pub fn find_package_template<S>(package: Option<&S>) -> Result<FindPackageTemplateResult>
where
    S: AsRef<str>,
{
    use FindPackageTemplateResult::*;

    // Lazily process the glob. This closure is expected to be called at
    // most once, but there are two code paths that might need to call it.
    let find_packages = || {
        glob::glob("*.spk.yaml")
            .into_diagnostic()?
            .collect::<std::result::Result<Vec<_>, _>>()
            .into_diagnostic()
            .wrap_err("Failed to discover spec files in current directory")
    };

    // This must catch and convert all the errors into the appropriate
    // FindPackageTemplateResult, e.g. NotFound(error_message), so
    // that find_package_recipe_from_template_or_repo() can operate
    // correctly.
    let package = match package {
        None => {
            let mut packages = find_packages()?;
            return match packages.len() {
                1 => {
                    let path = packages.pop().unwrap();
                    let template = match SpecTemplate::from_file(&path) {
                        Ok(t) => t,
                        Err(spk_schema::Error::InvalidPath(_, err)) => {
                            return Ok(NotFound(format!("{err}")));
                        }
                        Err(spk_schema::Error::FileOpenError(_, err)) => {
                            return Ok(NotFound(format!("{err}")));
                        }
                        Err(err) => {
                            return Err(err.into());
                        }
                    };
                    Ok(Found {
                        path,
                        template: Arc::new(template),
                    })
                }
                2.. => Ok(MultipleTemplateFiles(packages)),
                _ => Ok(NoTemplateFiles),
            };
        }
        Some(package) => package,
    };

    match SpecTemplate::from_file(package.as_ref().as_ref()) {
        Err(spk_schema::Error::InvalidPath(_, _err)) => {}
        Err(spk_schema::Error::FileOpenError(_, err))
            if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => {
            return Err(err.into());
        }
        Ok(res) => {
            return Ok(Found {
                path: package.as_ref().into(),
                template: Arc::new(res),
            });
        }
    }

    for path in find_packages()? {
        let template = match SpecTemplate::from_file(&path) {
            Ok(t) => t,
            Err(spk_schema::Error::InvalidPath(_, _)) => {
                continue;
            }
            Err(spk_schema::Error::FileOpenError(_, _)) => {
                continue;
            }
            Err(err) => {
                return Err(err.into());
            }
        };
        if template.name().as_str() == package.as_ref() {
            return Ok(Found {
                path,
                template: Arc::new(template),
            });
        }
    }

    Ok(NotFound(package.as_ref().to_owned()))
}

/// Find a package recipe either from a template file in the current
/// directory, or published version of the requested package, if any.
///
/// This function tries to discover the matching yaml template file
/// and populate it using the options. If it cannot find a file, it
/// will try to find the matching package/version in the repo and use
/// the recipe published for that.
///
pub async fn find_package_recipe_from_template_or_repo<S>(
    package_name: Option<&S>,
    options: &OptionMap,
    repos: &[Arc<storage::RepositoryHandle>],
) -> Result<(Arc<SpecRecipe>, std::path::PathBuf)>
where
    S: AsRef<str>,
{
    match find_package_template(package_name).wrap_err_with(|| {
        format!(
            "finding package template for {package_name}",
            package_name = {
                match &package_name {
                    Some(package_name) => package_name.as_ref(),
                    None => "something named *.spk.yaml in the current directory",
                }
            }
        )
    })? {
        FindPackageTemplateResult::Found { path, template } => {
            let recipe = template.render(options)?;
            Ok((Arc::new(recipe), path))
        }
        FindPackageTemplateResult::MultipleTemplateFiles(files) => {
            // must_be_found() will exit the program when called on MultipleTemplateFiles
            FindPackageTemplateResult::MultipleTemplateFiles(files).must_be_found();
            unreachable!()
        }
        FindPackageTemplateResult::NoTemplateFiles | FindPackageTemplateResult::NotFound(_) => {
            // If couldn't find a template file, maybe there's an
            // existing package/version that's been published
            match package_name {
                Some(name) => {
                    tracing::debug!("Unable to find package file: {}", name.as_ref());
                    // there will be at least one item for any string
                    let name_version = name.as_ref().split('@').next().unwrap();

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
                            match repo.read_recipe(pkg.as_version()).await {
                                Ok(recipe) => {
                                    tracing::debug!(
                                        "Using recipe found for {}",
                                        recipe.ident().format_ident(),
                                    );
                                    return Ok((recipe, std::path::PathBuf::from(&name.as_ref())));
                                }

                                Err(spk_storage::Error::PackageNotFound(_)) => continue,
                                Err(err) => return Err(err.into()),
                            }
                        }
                    }

                    miette::bail!(
                        help = "Check that file path, or package/version request, is correct",
                        "Unable to find {:?} as a file, or existing package/version recipe in any repo",
                        name.as_ref()
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
    }
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
    /// unless overridden with --solver-to-show
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

    /// Control what solver(s) are used.
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
    /// Control what solver's output is shown when multiple solvers
    /// (all) are being run.
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

    /// Set to capture each solver's output to a separate file in each
    /// time a solver is run. The files will be in the the given
    /// directory and named
    /// `<solver_file_prefix>_YYYYmmdd_HHMMSS_nnnnnnnn_<solver_kind>`. See
    /// --output-file-prefix for the default prefix and how to override it.
    #[clap(long, env = SPK_OUTPUT_TO_DIR, value_hint = ValueHint::FilePath)]
    output_to_dir: Option<std::path::PathBuf>,

    /// Set the minimum verbosity for solvers when outputting to a
    /// file. Has no affect unless --output-to-file is also specified.
    /// Verbosity set (-v) higher than this minimum will override it.
    #[clap(long, default_value_t=2, env = SPK_OUTPUT_TO_DIR_MIN_VERBOSITY)]
    output_to_dir_min_verbosity: u8,

    /// Override the default solver output filename prefix. The
    /// current date, time, and solver kind name will be appended to
    /// this prefix to produce the file name for each solver.
    #[clap(long, default_value_t=String::from(DEFAULT_SOLVER_RUN_FILE_PREFIX), env = SPK_OUTPUT_FILE_PREFIX)]
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
