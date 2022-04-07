// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::ffi::OsString;

use anyhow::{Context, Result};
use clap::Args;
use nix::unistd::ResUid;

use super::flags;

/// Remove a package from a repository
#[derive(Args)]
#[clap(visible_alias = "rm")]
pub struct Remove {
    #[clap(flatten)]
    pub solver: flags::Solver,
    #[clap(flatten)]
    pub options: flags::Options,
    #[clap(flatten)]
    pub runtime: flags::Runtime,
    #[clap(flatten)]
    pub requests: flags::Requests,

    #[clap(short, long, global = true, parse(from_occurrences))]
    pub verbose: u32,
    // remove_cmd = sub_parsers.add_parser(
    //     "remove",
    //     aliases=["rm"],
    //     help=_remove.__doc__,
    //     description=_remove.__doc__,
    //     **parser_args,
    // )
    // remove_cmd.add_argument(
    //     "--yes", action="store_true", help="Do not ask for confirmations (dangerous!)"
    // )
    // remove_cmd.add_argument(
    //     "packages", metavar="PKG", nargs="+", help="The packages to remove"
    // )
    // _flags.add_repo_flags(remove_cmd, defaults=[])
}

impl Remove {
    pub fn run(&self) -> Result<i32> {
        // repos = _flags.get_repos_from_repo_flags(args)
        // if not repos:
        //     print(
        //         f"{Fore.YELLOW}No repositories selected, specify --local-repo (-l) and/or --enable-repo (-r){Fore.RESET}",
        //         file=sys.stderr,
        //     )
        //     sys.exit(1)

        // for name in args.packages:

        //     if "/" not in name and not args.yes:
        //         answer = input(
        //             f"{Fore.YELLOW}Are you sure that you want to remove all versions of {name}?{Fore.RESET} [y/N]: "
        //         )
        //         if answer.lower() not in ("y", "yes"):
        //             sys.exit(1)

        //     for repo_name, repo in repos.items():

        //         if "/" not in name:
        //             versions = list(f"{name}/{v}" for v in repo.list_package_versions(name))
        //         else:
        //             versions = [name]

        //         for version in versions:

        //             ident = spk.api.parse_ident(version)
        //             if ident.build is not None:
        //                 _remove_build(repo_name, repo, ident)
        //             else:
        //                 _remove_all(repo_name, repo, ident)
        todo!()
    }
}

fn remove_build(
    repo_name: &String,
    repo: &spk::storage::RepositoryHandle,
    ident: &spk::api::Ident,
) -> Result<()> {
    // try:
    //     repo.remove_spec(ident)
    //     _LOGGER.info("removed build spec", pkg=ident, repo=repo_name)
    // except spk.storage.PackageNotFoundError:
    //     _LOGGER.warning("spec not found", pkg=ident, repo=repo_name)
    //     pass
    // try:
    //     repo.remove_package(ident)
    //     _LOGGER.info("removed build", pkg=ident, repo=repo_name)
    // except spk.storage.PackageNotFoundError:
    //     _LOGGER.warning("build not found", pkg=ident, repo=repo_name)
    //     pass
    todo!()
}

fn remove_all(
    repo_name: &String,
    repo: &spk::storage::RepositoryHandle,
    ident: &spk::api::Ident,
) -> Result<()> {
    // for build in repo.list_package_builds(ident):
    //     _remove_build(repo_name, repo, build)
    // try:
    //     repo.remove_spec(ident)
    //     _LOGGER.info("removed spec", pkg=ident, repo=repo_name)
    // except spk.storage.PackageNotFoundError:
    //     _LOGGER.warning("spec not found", pkg=ident, repo=repo_name)
    //     pass
    todo!()
}
