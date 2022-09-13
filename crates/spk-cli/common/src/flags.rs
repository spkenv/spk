// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;

use anyhow::{anyhow, bail, Context, Result};
use clap::Args;
use colored::Colorize;
use solve::{DecisionFormatter, DecisionFormatterBuilder};
use spk_schema::foundation::format::FormatIdent;
use spk_schema::foundation::ident_build::Build;
use spk_schema::foundation::ident_component::Component;
use spk_schema::foundation::name::{OptName, OptNameBuf};
use spk_schema::foundation::option_map::{host_options, OptionMap};
use spk_schema::foundation::spec_ops::Named;
use spk_schema::foundation::version::CompatRule;
use spk_schema::ident::{parse_ident, AnyIdent, PkgRequest, Request, RequestedBy, VarRequest};
use spk_schema::{Recipe, SpecRecipe, SpecTemplate, Template, TemplateExt, TestStage};
use spk_solve::{self as solve};
use spk_storage::{self as storage};

#[cfg(test)]
#[path = "./flags_test.rs"]
mod flags_test;

static SPK_NO_RUNTIME: &str = "SPK_NO_RUNTIME";

#[derive(Args, Clone)]
pub struct Runtime {
    /// Reconfigure the current spfs runtime (useful for speed and debugging)
    #[clap(long, env = SPK_NO_RUNTIME)]
    pub no_runtime: bool,

    /// A name to use for the created spfs runtime (useful for rejoining it later)
    #[clap(long)]
    pub env_name: Option<String>,
}

impl Runtime {
    /// Unless `--no-runtime` is present, relaunch the current process inside
    /// a new spfs runtime.
    ///
    /// The caller is expected to pass in a list of subcommand alises that can
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
            .context("One or more arguments was not a valid c-string")?;
        if !found_insertion_index {
            args.push(std::ffi::CString::new("--no-runtime").expect("--no-runtime is valid UTF-8"));
        }
        args.insert(0, std::ffi::CString::new("--").expect("should never fail"));
        args.insert(0, std::ffi::CString::new("-").expect("should never fail"));
        args.insert(0, std::ffi::CString::new("run").expect("should never fail"));
        args.insert(0, spfs.clone());

        tracing::debug!("relaunching under spfs");
        tracing::trace!("{:?}", args);
        nix::unistd::execvp(&spfs, args.as_slice())
            .context("Failed to re-launch spk in an spfs runtime")?;
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
    #[clap(long, env = "SPK_USE_IMPOSSIBLE_INITIAL")]
    pub impossible_initial: bool,

    /// If true, the solver will run impossible request checks before
    /// using a package build to resolve a request
    #[clap(long, env = "SPK_USE_IMPOSSIBLE_VALIDATION")]
    pub impossible_validation: bool,

    /// If true, the solver will run impossible request checks to
    /// use in the build keys for ordering builds during the solve
    #[clap(long, env = "SPK_USE_IMPOSSIBLE_BUILD_KEYS")]
    pub impossible_build_keys: bool,

    /// If true, the solver will run all three impossible request checks: initial
    /// requests, build validation before a resolve, and for build keys
    #[clap(long, env = "SPK_USE_IMPOSSIBLE_ALL_CHECKS")]
    pub impossible_all_checks: bool,
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
            self.impossible_initial || self.impossible_all_checks,
        );
        solver.set_resolve_validation_impossible_checks(
            self.impossible_validation || self.impossible_all_checks,
        );
        solver.set_build_key_impossible_checks(
            self.impossible_build_keys || self.impossible_all_checks,
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
            false => host_options().context("Failed to compute options for current host")?,
        };

        for req in self.get_var_requests()? {
            opts.insert(req.var, req.value);
        }

        Ok(opts)
    }

    pub fn get_var_requests(&self) -> Result<Vec<VarRequest>> {
        let mut requests = Vec::with_capacity(self.options.len());
        for pair in self.options.iter() {
            let pair = pair.trim();
            if pair.starts_with('{') {
                let given: HashMap<OptNameBuf, String> = serde_yaml::from_str(pair)
                    .context("--opt value looked like yaml, but could not be parsed")?;
                for (name, value) in given.into_iter() {
                    requests.push(VarRequest::new_with_value(name, value));
                }
                continue;
            }

            let (name, value) = pair
                .split_once('=')
                .or_else(|| pair.split_once(':'))
                .ok_or_else(|| {
                    anyhow!("Invalid option: -o {pair} (should be in the form name=value)")
                })
                .and_then(|(name, value)| Ok((OptName::new(name)?, value)))?;

            requests.push(VarRequest::new_with_value(name, value));
        }
        Ok(requests)
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
                let (recipe, _, stage) = parse_stage_specifier(package, options, repos).await?;

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
                let (_, template) = find_package_template(&Some(package))?.must_be_found();
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
        let var_requests = options.get_var_requests()?;
        let mut options = match options.no_host {
            true => OptionMap::default(),
            false => host_options()?,
        };
        // Insert var_requests, which includes requests specified on the command-line,
        // into the map so that they can override values provided by host_options().
        for req in var_requests {
            options.insert(req.var, req.value);
        }

        for (name, value) in options.iter() {
            if !value.is_empty() {
                out.push(VarRequest::new_with_value(name.clone(), value).into());
            }
        }

        for r in requests.into_iter() {
            let r = r.as_ref();
            if r.contains('@') {
                let (recipe, _, stage) = parse_stage_specifier(r, &options, repos).await?;

                match stage {
                    TestStage::Sources => {
                        let ident = recipe.ident().to_any(Some(Build::Source));
                        out.push(PkgRequest::from_ident(ident, RequestedBy::CommandLine).into());
                    }

                    TestStage::Build => {
                        let requirements = recipe.get_build_requirements(&options)?;
                        out.extend(requirements);
                    }
                    TestStage::Install => out.push(
                        PkgRequest::from_ident_exact(
                            recipe.ident().to_any(None),
                            RequestedBy::CommandLine,
                        )
                        .into(),
                    ),
                }
                continue;
            }
            let value: serde_yaml::Value =
                serde_yaml::from_str(r).context("Request was not a valid yaml value")?;
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
                .context(format!("Failed to parse request {r}"))?
                .into_pkg()
                .context(format!("Expected a package request, got {r}"))?;
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
}

/// Returns the spec, filename and stage for the given specifier
pub async fn parse_stage_specifier(
    specifier: &str,
    options: &OptionMap,
    repos: &[Arc<storage::RepositoryHandle>],
) -> Result<(Arc<SpecRecipe>, std::path::PathBuf, TestStage)> {
    let (package, stage) = specifier.split_once('@').ok_or_else(|| {
        anyhow!(
            "Package stage '{specifier}' must contain an '@' character (eg: @build, my-pkg@install)"
        )
    })?;

    let stage = TestStage::from_str(stage)?;

    let (spec, filename) =
        find_package_recipe_from_template_or_repo(&Some(package), options, repos).await?;

    Ok((spec, filename, stage))
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
pub fn find_package_template<S>(package: &Option<S>) -> Result<FindPackageTemplateResult>
where
    S: AsRef<str>,
{
    use FindPackageTemplateResult::*;

    // Lazily process the glob. This closure is expected to be called at
    // most once, but there are two code paths that might need to call it.
    let find_packages = || {
        glob::glob("*.spk.yaml")?
            .collect::<std::result::Result<Vec<_>, _>>()
            .context("Failed to discover spec files in current directory")
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
/// and populate it using the options. If it cannot file a file, it
/// will try to find the matching package/version in the repo and use
/// the recipe published for that.
///
pub async fn find_package_recipe_from_template_or_repo<S>(
    package_name: &Option<S>,
    options: &OptionMap,
    repos: &[Arc<storage::RepositoryHandle>],
) -> Result<(Arc<SpecRecipe>, std::path::PathBuf)>
where
    S: AsRef<str>,
{
    match find_package_template(package_name)? {
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

                    let pkg = parse_ident(name_version)?;
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
                            Err(spk_storage::Error::SpkValidatorsError(
                                spk_schema::validators::Error::PackageNotFoundError(_),
                            )) => continue,
                            Err(err) => return Err(err.into()),
                        }
                    }

                    tracing::error!(
                        "Unable to find {:?} as a file, or existing package/version recipe in any repo",
                        name.as_ref()
                    );
                    anyhow::bail!(
                        " > Please check that file path, or package/version request, is correct"
                    );
                }
                None => {
                    tracing::error!("Unable to find a spec file, or existing package/version");
                    anyhow::bail!(" > Please provide a file path, or package/version request");
                }
            }
        }
    }
}

#[derive(Args, Clone)]
pub struct Repositories {
    /// Enable the local repository (DEPRECATED)
    ///
    /// This option is ignored and the local repository is enabled by default.
    /// Use `--no-local-repo` to disable the local repository.
    #[clap(short, long, value_parser = warn_local_flag_deprecated)]
    pub local_repo: bool,

    /// Disable the local repository
    #[clap(long, hide = true)]
    pub no_local_repo: bool,

    /// Repositories to enable for the command
    ///
    /// Any configured spfs repository can be named here as well as "local" or
    /// a path on disk or a full remote repository url.
    #[clap(long, short = 'r')]
    pub enable_repo: Vec<String>,

    /// Repositories to exclude from the command
    ///
    /// Any configured spfs repository can be named here as well as "local"
    #[clap(long)]
    pub disable_repo: Vec<String>,
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
        let mut repos = Vec::new();
        if !self.no_local_repo
            && self.enable_repo.is_empty()
            // Interpret `--disable-repo local` as a request to not use the
            // local repo.
            && !self.disable_repo.iter().any(|s| s == "local")
        {
            let repo = storage::local_repository().await?;
            repos.push(("local".into(), repo.into()));
        }
        for name in self.enable_repo.iter() {
            if self.disable_repo.contains(name) {
                continue;
            }
            if repos.iter().any(|(s, _)| s == name) {
                // Already added
                continue;
            }

            let repo = match name.as_str() {
                // Allow `--enable-repo local` to work to enable the local repo.
                "local" => storage::local_repository().await,
                name => storage::remote_repository(name).await,
            }?;
            repos.push((name.into(), repo.into()));
        }
        Ok(repos)
    }

    /// Get the repositories to use based on command-line options.
    ///
    /// This method enables the "local" and "origin" repositories by default.
    /// This behavior can be altered with the `--enable-repo`, `--disable-repo`,
    /// and `--no-local-repo` flags.
    ///
    /// For backwards compatibility purposes, if the deprecated `--local-repo`
    /// flag is used, then only the local repo is enabled.
    ///
    /// The `--enable-repo` is considered additive instead of exclusive.
    ///
    /// Remote repos enabled with `--enable-repo` are added to the list before
    /// "origin".
    pub async fn get_repos_for_non_destructive_operation(
        &self,
    ) -> Result<Vec<(String, storage::RepositoryHandle)>> {
        let mut repos = Vec::new();
        if !self.no_local_repo
            // Interpret `--disable-repo local` as a request to not use the
            // local repo.
            && !self.disable_repo.iter().any(|s| s == "local")
        {
            let repo = storage::local_repository().await?;
            repos.push(("local".into(), repo.into()));
        }
        if self.local_repo {
            return Ok(repos);
        }
        for name in self
            .enable_repo
            .iter()
            .map(|s| s.as_ref())
            .chain(std::iter::once("origin"))
        {
            if self.disable_repo.iter().any(|s| s == name) {
                continue;
            }
            if repos.iter().any(|(s, _)| s == name) {
                // Already added
                continue;
            }

            let repo = match name {
                // Allow `--enable-repo local` to work to enable the local repo.
                "local" => storage::local_repository().await,
                name => storage::remote_repository(name).await,
            }?;
            repos.push((name.into(), repo.into()));
        }
        Ok(repos)
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
    #[clap(long, env = "SPK_SOLVE_TOO_LONG_SECONDS", default_value_t = 30)]
    pub increase_verbosity: u64,

    /// The maximum verbosity that automatic verbosity increases will
    /// stop at and not go above.
    ///
    #[clap(long, env = "SPK_VERBOSITY_INCREASE_LIMIT", default_value_t = 2)]
    pub max_verbosity_increase_level: u32,

    /// Maximum number of seconds to let the solver run before halting the solve
    ///
    /// Maximum number of seconds to alow a solver to run before
    /// halting the solve. If this is zero, which is the default, the
    /// timeout is disabled and the solver will run to completion.
    #[clap(long, env = "SPK_SOLVE_TIMEOUT", default_value_t = 0)]
    pub timeout: u64,

    /// Show the package builds in the solution for any solver
    /// run. This will be automatically enabled for 'build',
    /// 'make-binary', and 'explain' commands or if v > 0.
    #[clap(long)]
    pub show_solution: bool,

    /// Set the threshold of a longer than acceptable solves, in seconds.
    ///
    #[clap(long, env = "SPK_LONG_SOLVE_THRESHOLD", default_value_t = 15)]
    pub long_solves: u64,

    /// Set the limit for how many of the most frequent errors are
    /// displayed in solve stats reports
    #[clap(long, env = "SPK_MAX_FREQUENT_ERRORS", default_value_t = 15)]
    pub max_frequent_errors: usize,

    /// Display a visualization of the solver progress if the solve takes longer
    /// than a few seconds.
    #[clap(long)]
    pub status_bar: bool,
}

impl DecisionFormatterSettings {
    /// Get a decision formatter configured from the command line
    /// options and their defaults.
    pub fn get_formatter(&self, verbosity: u32) -> DecisionFormatter {
        self.get_formatter_builder(verbosity).build()
    }

    /// Get a decision formatter builder configured from the command
    /// line options and defaults and ready to call build() on, in
    /// case some extra configuration might be needed before calling
    /// build.
    pub fn get_formatter_builder(&self, verbosity: u32) -> DecisionFormatterBuilder {
        let mut builder = DecisionFormatterBuilder::new();
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
            .with_status_bar(self.status_bar);
        builder
    }
}

fn warn_local_flag_deprecated(arg: &str) -> Result<bool> {
    // This will be called with `"true"` if the flag is present on the command
    // line.
    if arg == "true" {
        // Logging is not configured at this point (args have to parsed to
        // know verbosity level before logging can be configured).
        eprintln!(
            "{warning}: The -l (--local-repo) is deprecated, please remove it from your command line!",
            warning = "WARNING".yellow());
        Ok(true)
    } else {
        Ok(false)
    }
}
