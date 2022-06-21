// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::{collections::HashMap, str::FromStr, sync::Arc};

use anyhow::{anyhow, bail, Context, Result};
use clap::Args;
use colored::Colorize;

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
    pub fn ensure_active_runtime(&self) -> Result<spfs::runtime::Runtime> {
        if self.no_runtime {
            return Ok(spk::HANDLE.block_on(spfs::active_runtime())?);
        }
        self.relaunch_with_runtime()
    }

    #[cfg(target_os = "linux")]
    pub fn relaunch_with_runtime(&self) -> Result<spfs::runtime::Runtime> {
        use std::os::unix::ffi::OsStrExt;

        let args = std::env::args_os();

        // ensure that we don't go into an infinite loop
        // by disabling this process in the next command
        std::env::set_var(SPK_NO_RUNTIME, "true");

        let spfs = std::ffi::CString::new("spfs").expect("should never fail");
        let mut args = args
            .map(|arg| std::ffi::CString::new(arg.as_bytes()))
            .collect::<std::result::Result<Vec<_>, _>>()
            .context("One or more arguments was not a valid c-string")?;
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
}

impl Solver {
    pub fn get_solver(&self, options: &Options) -> Result<spk::solve::Solver> {
        let option_map = options.get_options()?;
        let mut solver = spk::Solver::default();
        solver.update_options(option_map);
        self.repos
            .configure_solver(&mut solver, &["origin".to_string()])?;
        solver.set_binary_only(!self.allow_builds);
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
    pub fn get_options(&self) -> Result<spk::api::OptionMap> {
        let mut opts = match self.no_host {
            true => spk::api::OptionMap::default(),
            false => {
                spk::api::host_options().context("Failed to compute options for current host")?
            }
        };

        for req in self.get_var_requests()? {
            opts.insert(req.var, req.value);
        }

        Ok(opts)
    }

    pub fn get_var_requests(&self) -> Result<Vec<spk::api::VarRequest>> {
        let mut requests = Vec::with_capacity(self.options.len());
        for pair in self.options.iter() {
            let pair = pair.trim();
            if pair.starts_with('{') {
                let given: HashMap<String, String> = serde_yaml::from_str(pair)
                    .context("--opt value looked like yaml, but could not be parsed")?;
                for (name, value) in given.into_iter() {
                    requests.push(spk::api::VarRequest::new_with_value(name, value));
                }
                continue;
            }

            let (name, value) = pair
                .split_once('=')
                .or_else(|| pair.split_once(':'))
                .ok_or_else(|| {
                    anyhow!("Invalid option: -o {pair} (should be in the form name=value)")
                })?;
            requests.push(spk::api::VarRequest::new_with_value(name, value));
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
    pub fn parse_idents<'a, I: IntoIterator<Item = &'a str>>(
        &self,
        packages: I,
    ) -> Result<Vec<spk::api::Ident>> {
        let mut idents = Vec::new();
        for package in packages {
            if package.contains('@') {
                let (spec, _, stage) = parse_stage_specifier(package)?;

                match stage {
                    spk::api::TestStage::Sources => {
                        let ident = spec.pkg.with_build(Some(spk::api::Build::Source));
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
                let (_, spec) = find_package_spec(&Some(package))?.must_be_found();
                idents.push(spec.pkg.clone());
            } else {
                idents.push(spk::api::parse_ident(package)?)
            }
        }

        Ok(idents)
    }

    /// Parse and build a request from the given string and these flags
    pub fn parse_request<R: AsRef<str>>(
        &self,
        request: R,
        options: &Options,
    ) -> Result<spk::api::Request> {
        Ok(self
            .parse_requests([request.as_ref()], options)?
            .pop()
            .unwrap())
    }

    /// Parse and build requests from the given strings and these flags.
    pub fn parse_requests<I, S>(
        &self,
        requests: I,
        options: &Options,
    ) -> Result<Vec<spk::api::Request>>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let options = options.get_options()?;

        let mut out = Vec::<spk::api::Request>::new();
        for (name, value) in spk::api::host_options()?.into_iter() {
            if !value.is_empty() {
                out.push(spk::api::VarRequest::new_with_value(name, value).into());
            }
        }

        for r in requests.into_iter() {
            let r = r.as_ref();
            if r.contains('@') {
                let (spec, _, stage) = parse_stage_specifier(r)?;

                match stage {
                    spk::api::TestStage::Sources => {
                        let ident = spec.pkg.with_build(Some(spk::api::Build::Source));
                        out.push(
                            spk::api::PkgRequest::from_ident(
                                ident,
                                spk::api::RequestedBy::CommandLine,
                            )
                            .into(),
                        );
                    }

                    spk::api::TestStage::Build => {
                        let requirements =
                            spk::build::BinaryPackageBuilder::from_spec((*spec).clone())
                                .with_options(options.clone())
                                .get_build_requirements()?;
                        for request in requirements {
                            out.push(request);
                        }
                    }
                    spk::api::TestStage::Install => {
                        for request in spec.install.requirements.iter() {
                            let req = match request {
                                v @ spk::api::Request::Var(_) => v.clone(),
                                spk::api::Request::Pkg(r) => {
                                    let mut t = r.clone();
                                    t.add_requester(spk::api::RequestedBy::PackageBuild(
                                        spec.pkg.clone(),
                                    ));
                                    spk::api::Request::Pkg(t)
                                }
                            };
                            out.push(req);
                        }
                    }
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

            let mut req: spk::api::PkgRequest = serde_yaml::from_value(request_data.into())
                .context(format!("Failed to parse request {}", r))?;
            req.add_requester(spk::api::RequestedBy::CommandLine);

            if req.pkg.components.is_empty() {
                req.pkg.components.insert(spk::api::Component::Run);
            }
            if req.required_compat.is_none() {
                req.required_compat = Some(spk::api::CompatRule::API);
            }
            out.push(req.into());
        }

        Ok(out)
    }
}

/// Returns the spec, filename and stage for the given specifier
pub fn parse_stage_specifier(
    specifier: &str,
) -> Result<(Arc<spk::api::Spec>, std::path::PathBuf, spk::api::TestStage)> {
    let (package, stage) = specifier.split_once('@').ok_or_else(|| {
        anyhow!(
            "Package stage '{specifier}' must contain an '@' character (eg: @build, my-pkg@install)"
        )
    })?;

    let stage = spk::api::TestStage::from_str(stage)?;

    let (filename, spec) = find_package_spec(&Some(package))?.must_be_found();
    Ok((spec, filename, stage))
}

/// The result of the [`find_package_spec`] function.
// We are okay with the large variant here because it's specifically
// used as the positive result of the function, with the others simply
// denoting unique error cases.
#[allow(clippy::large_enum_variant)]
pub enum FindPackageSpecResult {
    /// A non-ambiguous package spec file was found
    Found {
        path: std::path::PathBuf,
        spec: Arc<spk::api::Spec>,
    },
    /// No package was specifically requested, and there are multiple
    /// spec files in the current repository.
    MultipleSpecFiles,
    /// No package was specifically requested, and there are multiple
    /// spec files in the current repository.
    NoSpecFiles,
    NotFound(String),
}

impl FindPackageSpecResult {
    pub fn is_found(&self) -> bool {
        matches!(self, Self::Found { .. })
    }

    /// Prints error messages and exists if no spec file was found
    pub fn must_be_found(self) -> (std::path::PathBuf, Arc<spk::api::Spec>) {
        match self {
            Self::Found { path, spec } => return (path, spec),
            Self::MultipleSpecFiles => {
                tracing::error!("Multiple package specs in current directory");
                tracing::error!(" > please specify a package name or filepath");
            }
            Self::NoSpecFiles => {
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

/// Find a package spec file for the requested package, if any.
///
/// This function will use the current directory and the provided
/// package name or filename to try and discover the matching
/// yaml spec file.
pub fn find_package_spec<S>(package: &Option<S>) -> Result<FindPackageSpecResult>
where
    S: AsRef<str>,
{
    use FindPackageSpecResult::*;

    // Lazily process the glob. This closure is expected to be called at
    // most once, but there are two code paths that might need to call it.
    let find_packages = || {
        glob::glob("*.spk.yaml")?
            .collect::<std::result::Result<Vec<_>, _>>()
            .context("Failed to discover spec files in current directory")
    };

    let package = match package {
        None => {
            let mut packages = find_packages()?;

            return match packages.len() {
                1 => {
                    let path = packages.pop().unwrap();
                    let spec = Arc::new(spk::api::read_spec_file(&path)?);
                    Ok(Found { path, spec })
                }
                2.. => Ok(MultipleSpecFiles),
                _ => Ok(NoSpecFiles),
            };
        }
        Some(package) => package,
    };

    match spk::api::read_spec_file(package.as_ref()) {
        Err(spk::Error::IO(err)) if err.kind() == std::io::ErrorKind::NotFound => {}
        res => {
            return Ok(Found {
                path: package.as_ref().into(),
                spec: Arc::new(res?),
            })
        }
    }

    for path in find_packages()? {
        let spec = spk::api::read_spec_file(&path)?;
        if spec.pkg.name.as_str() == package.as_ref() {
            return Ok(Found {
                path,
                spec: Arc::new(spec),
            });
        }
    }

    Ok(NotFound(package.as_ref().to_owned()))
}

#[derive(Args, Clone)]
pub struct Repositories {
    /// Resolve packages from the local repository (DEPRECATED)
    ///
    /// This option is ignored and the local repository is enabled by default.
    /// Use `--no-local-repo` to disable the local repository.
    #[clap(short, long, value_parser = warn_local_flag_deprecated)]
    pub local_repo: bool,

    /// Disable resolving packages from the local repository
    #[clap(long)]
    pub no_local_repo: bool,

    /// Repositories to include in the resolve
    ///
    /// Any configured spfs repository can be named here as well as a path
    /// on disk or a full remote repository url.
    #[clap(long, short = 'r')]
    pub enable_repo: Vec<String>,

    /// Repositories to exclude in the resolve
    ///
    /// Any configured spfs repository can be named here
    #[clap(long)]
    pub disable_repo: Vec<String>,
}

impl Repositories {
    /// Configure a solver with the repositories requested on the command line.
    ///
    /// The provided defaults are used if nothing was specified.
    pub fn configure_solver<'a, 'b: 'a, I>(
        &'b self,
        solver: &mut spk::solve::Solver,
        defaults: I,
    ) -> Result<()>
    where
        I: IntoIterator<Item = &'a String>,
    {
        for (name, repo) in self.get_repos(defaults)?.into_iter() {
            tracing::debug!(repo=%name, "using repository");
            solver.add_repository(repo);
        }
        Ok(())
    }

    /// Get the repositories specified on the command line.
    ///
    /// The provided defaults are used if nothing was specified.
    pub fn get_repos<'a, 'b: 'a, I: IntoIterator<Item = &'a String>>(
        &'b self,
        defaults: I,
    ) -> Result<Vec<(String, spk::storage::RepositoryHandle)>> {
        let mut repos = Vec::new();
        if !self.no_local_repo {
            let repo = spk::HANDLE.block_on(spk::storage::local_repository())?;
            repos.push(("local".into(), repo.into()));
        }
        let enabled = self.enable_repo.iter().chain(defaults.into_iter());
        for name in enabled {
            if !self.disable_repo.contains(name) {
                let repo = spk::HANDLE.block_on(spk::storage::remote_repository(name))?;
                repos.push((name.into(), repo.into()));
            }
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

    /// Maximum number of seconds to let the solver run before halting the solve
    ///
    /// Maximum number of seconds to alow a solver to run before
    /// halting the solve. If this is zero, which is the default, the
    /// timeout is disabled and the solver will run to completion.
    #[clap(long, env = "SPK_SOLVE_TIMEOUT", default_value_t = 0)]
    pub timeout: u64,
}

impl DecisionFormatterSettings {
    /// Get a decision formatter configured from the command line
    /// options and their defaults.
    pub fn get_formatter(&self, verbosity: u32) -> spk::io::DecisionFormatter {
        spk::io::DecisionFormatterBuilder::new()
            .with_verbosity(verbosity)
            .with_time_and_stats(self.time)
            .with_verbosity_increase_every(self.increase_verbosity)
            .with_timeout(self.timeout)
            .build()
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
    }
    Ok(true)
}
