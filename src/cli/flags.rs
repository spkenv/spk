// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::HashMap;

use anyhow::{anyhow, Context, Result};
use clap::Args;

static SPK_NO_RUNTIME: &str = "SPK_NO_RUNTIME";

// OPTION_VAR_RE = re.compile(r"^SPK_OPT_([\w\.]+)$")

#[derive(Args)]
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
            return Ok(spfs::active_runtime()?);
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

#[derive(Args)]
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

#[derive(Args)]
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

#[derive(Args)]
pub struct Requests {
    /// Allow pre-releases for all command line package requests
    #[clap(long)]
    pub pre: bool,
}

impl Requests {
    pub fn parse_idents<'a, I: IntoIterator<Item = &'a str>>(packages: I) -> Vec<spk::api::Ident> {
        // idents = []
        // for package in packages:
        //     if "@" in package:
        //         spec, _, stage = parse_stage_specifier(package)

        //         if stage == "source":
        //             ident = spec.pkg.with_build(spk::api::SRC)
        //             idents.append(ident)

        //         else:
        //             print(
        //                 f"Unsupported stage '{stage}', can only be empty or 'source' in this context"
        //             )
        //             sys.exit(1)

        //     if os.path.isfile(package):
        //         spec, _ = find_package_spec(package)
        //         idents.append(spec.pkg)

        //     else:
        //         idents.append(spk::api::parse_ident(package))

        // return idents
        todo!()
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
    pub fn parse_requests<'a, I: IntoIterator<Item = &'a str>>(
        &self,
        requests: I,
        options: &Options,
    ) -> Result<Vec<spk::api::Request>> {
        let options = options.get_options()?;

        let mut out = Vec::<spk::api::Request>::new();
        for (name, value) in spk::api::host_options()?.into_iter() {
            if !value.is_empty() {
                out.push(spk::api::VarRequest::new_with_value(name, value).into());
            }
        }

        for r in requests.into_iter() {
            if r.contains('@') {
                let (spec, _, stage) = parse_stage_specifier(r)?;

                match stage {
                    spk::api::TestStage::Sources => {
                        let ident = spec.pkg.with_build(Some(spk::api::Build::Source));
                        out.push(spk::api::PkgRequest::from_ident(&ident).into());
                    }

                    spk::api::TestStage::Build => {
                        let requirements = spk::build::BinaryPackageBuilder::from_spec(spec)
                            .with_options(options.clone())
                            .get_build_requirements()?;
                        for request in requirements {
                            out.push(request);
                        }
                    }
                    spk::api::TestStage::Install => {
                        for request in spec.install.requirements.iter() {
                            out.push(request.clone());
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
                    return Err(anyhow!(
                        "Invalid request, expected either a string or a mapping, got: {:?}",
                        value
                    ))
                }
            };

            let prerelease_policy_key = "prereleasePolicy".into();
            if self.pre && !request_data.contains_key(&prerelease_policy_key) {
                request_data.insert(prerelease_policy_key, "IncludeAll".into());
            }

            let mut req: spk::api::PkgRequest = serde_yaml::from_value(request_data.into())
                .context(format!("Failed to parse request {}", r))?;
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
) -> Result<(spk::api::Spec, String, spk::api::TestStage)> {
    //     if "@" not in specifier:
    //         raise ValueError(
    //             f"Package stage '{specifier}' must contain an '@' character (eg: @build, my-pkg@install)"
    //         )

    //     package, stage = specifier.split("@", 1)
    //     spec, filename = find_package_spec(package)
    //     return spec, filename, stage
    todo!()
}

// pub fn find_package_spec(package: str) -> Tuple[spk::api::Spec, str]:

//     packages = glob.glob("*.spk.yaml")
//     if not package:
//         if len(packages) == 1:
//             package = packages[0]
//         elif len(packages) > 1:
//             print(
//                 f"{Fore.RED}Multiple package specs in current directory{Fore.RESET}",
//                 file=sys.stderr,
//             )
//             print(
//                 f"{Fore.RED} > please specify a package name or filepath{Fore.RESET}",
//                 file=sys.stderr,
//             )
//             sys.exit(1)
//         else:
//             print(
//                 f"{Fore.RED}No package specs found in current directory{Fore.RESET}",
//                 file=sys.stderr,
//             )
//             print(
//                 f"{Fore.RED} > please specify a filepath{Fore.RESET}", file=sys.stderr
//             )
//             sys.exit(1)
//     try:
//         spec = spk::api::read_spec_file(package)
//     except FileNotFoundError:
//         for filename in packages:
//             spec = spk::api::read_spec_file(filename)
//             if spec.pkg.name == package:
//                 package = filename
//                 break
//         else:
//             raise
//     return spec, package

#[derive(Args)]
pub struct Repositories {
    /// Resolve packages from the local repository
    #[clap(short, long)]
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
        if self.local_repo && !self.no_local_repo {
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
