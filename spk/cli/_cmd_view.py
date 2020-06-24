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

    view_cmd = sub_parsers.add_parser(
        "view", aliases=["list"], help=_view.__doc__, **parser_args,
    )
    view_cmd.add_argument(
        "package",
        metavar="PKG",
        nargs="?",
        help="Package, or package build to show the spec file of",
    )
    _flags.add_repo_flags(view_cmd, defaults=[])
    view_cmd.set_defaults(func=_view)
    return view_cmd


def _view(args: argparse.Namespace) -> None:
    """view a package into a shared repository."""

    repos = _flags.get_repos_from_repo_flags(args)
    if not repos:
        print(
            f"{Fore.YELLOW}No repositories selected, specify --local-repo (-l) and/or --enable-repo (-r){Fore.RESET}",
            file=sys.stderr,
        )
        sys.exit(1)

    pkg = spk.api.parse_ident(args.package)
    for repo in repos.values():

        try:
            spec = repo.read_spec(pkg)
        except spk.storage.PackageNotFoundError:
            continue

        yaml.safe_dump(spec.to_dict(), sys.stdout)
        break
    else:
        raise spk.storage.PackageNotFoundError(pkg)
