from typing import Callable, Any
import argparse
import os
import sys
import termios

import spkrs
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
        "--yes", action="store_true", help="Do not ask for confirmations (dangerous!)"
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

        if "/" not in name and not args.yes:
            answer = input(
                f"{Fore.YELLOW}Are you sure that you want to remove all versions of {name}?{Fore.RESET} [y/N]: "
            )
            if answer.lower() not in ("y", "yes"):
                sys.exit(1)

        for repo_name, repo in repos.items():

            if "/" not in name:
                versions = list(f"{name}/{v}" for v in repo.list_package_versions(name))
            else:
                versions = [name]

            for version in versions:

                ident = spk.api.parse_ident(version)
                if ident.build is not None:
                    _remove_build(repo_name, repo, ident)
                else:
                    _remove_all(repo_name, repo, ident)


def _remove_build(
    repo_name: str, repo: spk.storage.Repository, ident: spk.api.Ident
) -> None:
    try:
        repo.remove_spec(ident)
        _LOGGER.info("removed build spec", pkg=ident, repo=repo_name)
    except spk.storage.PackageNotFoundError:
        _LOGGER.warning("spec not found", pkg=ident, repo=repo_name)
        pass
    try:
        repo.remove_package(ident)
        _LOGGER.info("removed build", pkg=ident, repo=repo_name)
    except spk.storage.PackageNotFoundError:
        _LOGGER.warning("build not found", pkg=ident, repo=repo_name)
        pass


def _remove_all(
    repo_name: str, repo: spk.storage.Repository, ident: spk.api.Ident
) -> None:

    for build in repo.list_package_builds(ident):
        _remove_build(repo_name, repo, build)
    try:
        repo.remove_spec(ident)
        _LOGGER.info("removed spec", pkg=ident, repo=repo_name)
    except spk.storage.PackageNotFoundError:
        _LOGGER.warning("spec not found", pkg=ident, repo=repo_name)
        pass
