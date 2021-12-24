# Copyright (c) 2021 Sony Pictures Imageworks, et al.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/imageworks/spk

from typing import Any, Dict
import argparse
import sys

import structlog
from colorama import Fore, Style

import spk

from . import _flags

_LOGGER = structlog.get_logger("spk.cli")


def register(
    sub_parsers: argparse._SubParsersAction, **parser_args: Any
) -> argparse.ArgumentParser:

    ls_cmd = sub_parsers.add_parser(
        "ls", aliases=["list"], help=_ls.__doc__, description=_ls.__doc__, **parser_args
    )
    ls_cmd.add_argument(
        "package",
        metavar="PKG",
        nargs="?",
        help="Given a name, list versions. Given a name/version list builds",
    )
    ls_cmd.add_argument(
        "--recursive",
        action="store_true",
        help="Recursively list all package versions and builds (recursive results are not sorted)",
    )
    ls_cmd.add_argument(
        "--components",
        "-c",
        action="store_true",
        help="Show available package components in the output",
    )
    # no defaults since we want --local to be mutually exclusive
    _flags.add_repo_flags(ls_cmd, defaults=[])
    ls_cmd.set_defaults(func=_ls)
    return ls_cmd


def _ls(args: argparse.Namespace) -> None:
    """List packages in one or more repositories."""

    repos = _flags.get_repos_from_repo_flags(args)
    if not repos:
        if "origin" not in args.disable_repo:
            args.enable_repo = ["origin"]
            repos = _flags.get_repos_from_repo_flags(args)
        else:
            print(
                f"{Fore.YELLOW}No repositories selected, specify --local-repo (-l) and/or --enable-repo (-r){Fore.RESET}",
                file=sys.stderr,
            )
            sys.exit(1)

    if args.recursive:
        return _list_recursively(repos, args)

    results = set()
    if not args.package:
        for repo_name, repo in repos.items():
            for name in repo.list_packages():
                results.add(name)

    elif "/" not in args.package:
        for repo in repos.values():
            for version in repo.list_package_versions(args.package):
                results.add(str(version))

    else:
        for repo in repos.values():
            pkg = spk.api.parse_ident(args.package)
            for build in repo.list_package_builds(pkg):
                if not build.build or build.build == spk.api.SRC:
                    results.add(spk.io.format_ident(build))
                    continue

                item = spk.io.format_ident(build)
                if args.verbose:
                    spec = repo.read_spec(build)
                    options = spec.resolve_all_options(spk.api.OptionMap({}))
                    item += " " + spk.io.format_options(options)
                if args.verbose > 1:
                    cmpts = repo.get_package(build).keys()
                    item += " " + spk.io.format_components(list(cmpts))
                results.add(item)

    print("\n".join(sorted(results)))


def _list_recursively(
    repos: Dict[str, spk.storage.Repository],
    args: argparse.Namespace,
) -> None:

    for _repo_name, repo in repos.items():
        if not args.package:
            packages = repo.list_packages()
        else:
            packages = [args.package]
        for package in packages:
            if "/" in package:
                versions = [package]
            else:
                versions = [
                    f"{package}/{v}" for v in repo.list_package_versions(package)
                ]
            for version in versions:
                pkg = spk.api.parse_ident(version)
                for build in repo.list_package_builds(pkg):
                    if not build.build or build.build == "SRC":
                        print(spk.io.format_ident(build))
                        continue

                    print(spk.io.format_ident(build), end="")
                    if args.verbose:
                        spec = repo.read_spec(build)
                        options = spec.resolve_all_options(spk.api.OptionMap({}))
                        print("", spk.io.format_options(options), end="")
                    if args.verbose > 1:
                        cmpts = repo.get_package(build).keys()
                        print("", spk.io.format_components(list(cmpts)), end="")
                    print()
