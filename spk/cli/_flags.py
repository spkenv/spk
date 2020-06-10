from typing import Dict, List
import argparse

import spk


def add_repo_flags(
    parser: argparse.ArgumentParser, defaults: List[str] = ["origin"]
) -> None:

    parser.add_argument(
        "--local-repo",
        "-l",
        action="store_true",
        help="Enable resolving packages from the local repository",
    )
    parser.add_argument(
        "--enable-repo",
        "-r",
        type=str,
        metavar="NAME",
        action="append",
        default=defaults,
        help="Repositories to include in the resolve. Any configured spfs repository can be named here",
    )


def add_option_flags(parser: argparse.ArgumentParser) -> None:

    parser.add_argument(
        "--opt",
        "-o",
        type=str,
        default=[],
        action="append",
        help="Specify option values for the envrionment",
    )


def get_options_from_flags(args: argparse.Namespace) -> spk.api.OptionMap:

    opts = spk.api.host_options()
    for pair in args.opt:

        name, value = pair.split("=")
        opts[name] = value

    return opts


def configure_solver_with_repo_flags(
    args: argparse.Namespace, solver: spk.Solver
) -> None:

    for repo in get_repos_from_repo_flags(args).values():
        solver.add_repository(repo)


def get_repos_from_repo_flags(
    args: argparse.Namespace,
) -> Dict[str, spk.storage.Repository]:

    repos: Dict[str, spk.storage.Repository] = {}
    if args.local_repo:
        repos["local"] = spk.storage.local_repository()
    for name in args.enable_repo:
        repos[name] = spk.storage.remote_repository(name)
    return repos
