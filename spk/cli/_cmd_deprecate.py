# Copyright (c) 2021 Sony Pictures Imageworks, et al.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/imageworks/spk

from typing import Any
import argparse
import sys

import structlog
from colorama import Fore

import spk

from . import _flags

_LOGGER = structlog.get_logger("spk.cli")


def register(
    sub_parsers: argparse._SubParsersAction, **parser_args: Any
) -> argparse.ArgumentParser:

    deprecate_cmd = sub_parsers.add_parser(
        "deprecate",
        help=_deprecate.__doc__,
        description=_deprecate.__doc__,
        **parser_args,
    )
    deprecate_cmd.add_argument(
        "packages", metavar="PKG", nargs="+", help="The packages to deprecate"
    )
    _flags.add_repo_flags(deprecate_cmd, defaults=[])
    deprecate_cmd.set_defaults(func=_deprecate)
    return deprecate_cmd


def _deprecate(args: argparse.Namespace) -> None:
    """deprecate a package in a repository.

    Deprecated packages can still be resolved by requesting the exact build,
    but will otherwise not show up in environments. By deprecating a package
    version, as opposed to an individual build, the package will also no longer
    be rebuilt from source under any circumstances. This also deprecates all builds
    by association.
    """

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
            spec = repo.read_spec(ident)
            spec.deprecated = True
            repo.force_publish_spec(spec)
            _LOGGER.info("deprecated", pkg=ident, repo=repo_name)
