// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::HashMap;

use clap::Args;

// OPTION_VAR_RE = re.compile(r"^SPK_OPT_([\w\.]+)$")

#[derive(Args)]
pub struct Runtime {
    /// Reconfigure the current spfs runtime (useful for speed and debugging)
    #[clap(long)]
    pub no_runtime: bool,

    /// A name to use for the created spfs runtime (useful for rejoining it later)
    #[clap(long)]
    pub env_name: Option<String>,
}

impl Runtime {
    pub fn ensure_active_runtime(&self) -> spfs::runtime::Runtime {
        // if args.no_runtime:
        //     return spkrs.active_runtime()

        // cmd = sys.argv
        // cmd_index = cmd.index(args.command)
        // cmd.insert(cmd_index + 1, "--no-runtime")
        // name_args = ["--name", args.env_name] if args.env_name else []
        // cmd = ["spfs", "run", *name_args, "-", "--"] + cmd
        // os.execvp(cmd[0], cmd)
        todo!()
    }
}

#[derive(Args)]
pub struct Solver {
    #[clap(flatten)]
    pub options: Options,
    #[clap(flatten)]
    pub repo: Repositories,

    /// If true, build packages from source if needed
    #[clap(long)]
    pub allow_builds: bool,
}

impl Solver {
    pub fn get_solver(&self) -> spk::solve::Solver {
        // options = get_options_from_flags(args)
        // solver = spk.Solver()
        // solver.update_options(options)
        // configure_solver_with_repo_flags(args, solver) // defaults = ["origin"]
        // solver.set_binary_only(not args.allow_builds)
        // for r in get_var_requests_from_option_flags(args):
        //     solver.add_request(r)
        // return solver
        todo!()
    }
}

#[derive(Args)]
pub struct Options {
    /// Specify build options
    #[clap(long = "opt", short)]
    pub options: Vec<String>,

    /// Do not add the default options for the current host system
    #[clap(long)]
    pub no_host: bool,
}

impl Options {
    pub fn get_options(&self) -> spk::api::OptionMap {
        // if args.no_host:
        //     opts = spk::api::OptionMap()
        // else:
        //     opts = spk::api::host_options()

        // for req in get_var_requests_from_option_flags(args):
        //     opts[req.var] = req.value

        // return opts
        todo!()
    }

    pub fn get_var_requests(&self) -> Vec<spk::api::VarRequest> {
        // for pair in getattr(args, "opt", []):

        //     pair = pair.strip()
        //     if pair.startswith("{"):
        //         for name, value in (yaml.safe_load(pair) or {}).items():
        //             assert not isinstance(
        //                 value, dict
        //             ), f"provided value for '{name}' must be a scalar"
        //             assert not isinstance(
        //                 value, list
        //             ), f"provided value for '{name}' must be a scalar"
        //             yield spk::api::VarRequest(name, str(value))
        //         continue

        //     if "=" in pair:
        //         name, value = pair.split("=", 1)
        //     elif ":" in pair:
        //         name, value = pair.split(":", 1)
        //     else:
        //         raise ValueError(
        //             f"Invalid option: -o {pair} (should be in the form name=value)"
        //         )
        //     yield spk::api::VarRequest(name, value)
        todo!()
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

    /// Parse an build a request from the given string and these flags
    pub fn parse_request<R: AsRef<str>>(&self, request: R) -> spk::api::Request {
        self.parse_requests([request.as_ref()]).pop().unwrap()
    }

    /// Parse an build requests from the given strings and these flags
    pub fn parse_requests<'a, I: IntoIterator<Item = &'a str>>(
        &self,
        requests: I,
    ) -> Vec<spk::api::Request> {
        // options = get_options_from_flags(args)

        // out: List[spk::api::Request] = []
        // for name, value in spk::api::host_options().items():
        //     if value:
        //         out.append(spk::api::VarRequest(name, value))

        // for r in requests:

        //     if "@" in r:
        //         spec, _, stage = parse_stage_specifier(r)

        //         if stage == "source":
        //             ident = spec.pkg.with_build(spk::api::SRC)
        //             out.append(spk::api::PkgRequest.from_ident(ident))

        //         elif stage == "build":
        //             builder = spk.build.BinaryPackageBuilder.from_spec(spec).with_options(
        //                 options
        //             )
        //             for request in builder.get_build_requirements():
        //                 out.append(request)

        //         elif stage == "install":
        //             for request in spec.install.requirements:
        //                 out.append(request)
        //         else:
        //             print(
        //                 f"Unknown stage '{stage}', should be one of: 'source', 'build', 'install'"
        //             )
        //             sys.exit(1)
        //     else:
        //         parsed = yaml.safe_load(r)
        //         if isinstance(parsed, str):
        //             request_data = {"pkg": parsed}
        //         else:
        //             request_data = parsed

        //         if args.pre:
        //             request_data.setdefault("prereleasePolicy", "IncludeAll")

        //         req = spk::api::PkgRequest.from_dict(request_data)
        //         if not req.pkg.components:
        //             pkg = req.pkg
        //             pkg.components = set(["run"])
        //             req.pkg = pkg
        //         if req.required_compat is None:
        //             req.required_compat = "API"
        //         out.append(req)

        // return out
        todo!()
    }
}

// pub fn parse_stage_specifier(specifier: str) -> Tuple[spk::api::Spec, str, str]:
//     """Returns the spec, filename and stage for the given specifier."""

//     if "@" not in specifier:
//         raise ValueError(
//             f"Package stage '{specifier}' must contain an '@' character (eg: @build, my-pkg@install)"
//         )

//     package, stage = specifier.split("@", 1)
//     spec, filename = find_package_spec(package)
//     return spec, filename, stage

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
    #[clap(short, long)]
    pub no_local_repo: bool,

    /// Repositories to include in the resolve
    ///
    /// Any configured spfs repository can be named here as well as a path
    /// on disk or a full remote repository url.
    #[clap(long, short)]
    pub enable_repo: Vec<String>,

    /// Repositories to exclude in the resolve
    ///
    /// Any configured spfs repository can be named here
    #[clap(long, short)]
    pub disable_repo: Vec<String>,
}

impl Repositories {
    pub fn configure_solver(&self, solver: &mut spk::solve::Solver) {
        // for name, repo in get_repos_from_repo_flags(args).items():
        //     _LOGGER.debug("using repository", repo=name)
        //     solver.add_repository(repo)
        todo!()
    }

    /// Get the repositories specified on the command line.
    ///
    /// The provided defaults are used if nothing was specified.
    pub fn get_repos<'a, I: IntoIterator<Item = &'a str>>(
        &self,
        defaults: I,
    ) -> HashMap<String, spk::storage::RepositoryHandle> {
        // repos: Dict[str, spk.storage.Repository] = OrderedDict()
        // if args.local_repo:
        //     repos["local"] = spk.storage.local_repository()
        // for name in args.enable_repo:
        //     if name not in args.disable_repo:
        //         repos[name] = spk.storage.remote_repository(name)
        // return repos
        todo!()
    }
}
