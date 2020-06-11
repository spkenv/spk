from typing import Callable, Any
import argparse
import os
import sys
import termios

import spfs
import structlog
from colorama import Fore, Style

import spk
from spk.io import format_ident

from . import _flags

_LOGGER = structlog.get_logger("spk.cli")


def register(
    sub_parsers: argparse._SubParsersAction, **parser_args: Any
) -> argparse.ArgumentParser:

    ls_cmd = sub_parsers.add_parser(
        "ls", aliases=["list"], help=_ls.__doc__, **parser_args,
    )
    ls_cmd.add_argument(
        "package",
        metavar="PKG",
        nargs="?",
        help="Given a name, list versions. Given a name/version list builds",
    )
    _flags.add_repo_flags(ls_cmd, defaults=[])
    ls_cmd.set_defaults(func=_ls)
    return ls_cmd


def _ls(args: argparse.Namespace) -> None:
    """ls a package into a shared repository."""

    repos = _flags.get_repos_from_repo_flags(args)
    if not repos:
        print(
            f"{Fore.YELLOW}No repositories selected, specify --local-repo and/or --enable-repo{Fore.RESET}",
            file=sys.stderr,
        )
        sys.exit(1)

    if not args.package:
        for repo in repos.values():
            for name in repo.list_packages():
                print(name)
            continue

    elif "/" not in args.package:
        for repo in repos.values():
            for version in repo.list_package_versions(args.package):
                print(version)
            continue

    else:
        for repo in repos.values():
            pkg = spk.api.parse_ident(args.package)
            for build in repo.list_package_builds(pkg):
                print(build)
