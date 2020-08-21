from typing import Callable, Any
import argparse
import os
import sys
import termios

import spfs
import structlog
from ruamel import yaml
from colorama import Fore, Style

import spk
from spk.io import format_ident

from . import _flags

_LOGGER = structlog.get_logger("spk.cli")


def register(
    sub_parsers: argparse._SubParsersAction, **parser_args: Any
) -> argparse.ArgumentParser:

    remove_cmd = sub_parsers.add_parser(
        "remove", aliases=["rm"], help=_remove.__doc__, **parser_args
    )
    remove_cmd.add_argument(
        "packages", metavar="PKG", nargs="+", help="The packages to remove"
    )
    _flags.add_repo_flags(remove_cmd, defaults=[])
    remove_cmd.set_defaults(func=_remove)
    return remove_cmd


def _remove(args: argparse.Namespace) -> None:
    """remove a package from a repository."""

    repos = _flags.get_repos_from_repo_flags(args)
    if not repos:
        print(
            f"{Fore.YELLOW}No repositories selected, specify --local-repo (-l) and/or --enable-repo (-r){Fore.RESET}",
            file=sys.stderr,
        )
        sys.exit(1)

    for name in args.packages:
        if "/" not in name:
            print(f"{Fore.RED}Must provide a version number: {name}/???")
            print(f" > use 'spk ls {name}' to view available versions{Fore.RESET}")
            sys.exit(1)
        ident = spk.api.parse_ident(name)
        for repo_name, repo in repos.items():
            if ident.build is not None:
                try:
                    repo.remove_package(ident)
                    _LOGGER.info("removed build", pkg=ident, repo=repo_name)
                except spk.storage.PackageNotFoundError:
                    _LOGGER.warning("build not found", pkg=ident, repo=repo_name)
                    pass
                try:
                    repo.remove_package(ident)
                    _LOGGER.info("removed build spec", pkg=ident, repo=repo_name)
                except spk.storage.PackageNotFoundError:
                    _LOGGER.warning("spec not found", pkg=ident, repo=repo_name)
                    pass
            else:
                for build in repo.list_package_builds(ident):
                    try:
                        repo.remove_package(build)
                        _LOGGER.info("removed build", pkg=build, repo=repo_name)
                    except spk.storage.PackageNotFoundError:
                        _LOGGER.warning("build not found", pkg=ident, repo=repo_name)
                        pass
                    try:
                        repo.remove_package(build)
                        _LOGGER.info("removed build spec", pkg=build, repo=repo_name)
                    except spk.storage.PackageNotFoundError:
                        _LOGGER.warning("spec not found", pkg=ident, repo=repo_name)
                        pass
                try:
                    repo.remove_spec(ident)
                    _LOGGER.info("removed spec", pkg=ident, repo=repo_name)
                except spk.storage.PackageNotFoundError:
                    _LOGGER.warning("spec not found", pkg=ident, repo=repo_name)
                    pass
