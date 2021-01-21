from typing import Any, Dict
import argparse
import sys

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
        "ls", aliases=["list"], help=_ls.__doc__, **parser_args
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
        help="Recursively list all package versions and builds",
    )
    _flags.add_repo_flags(ls_cmd)
    ls_cmd.set_defaults(func=_ls)
    return ls_cmd


def _ls(args: argparse.Namespace) -> None:
    """list packages in one or more repositories."""

    prefix = f"{Style.DIM}{{repo}}:{Style.RESET_ALL} "
    end = "\n"
    if not sys.stdout.isatty():
        prefix = ""
        end = " "

    repos = _flags.get_repos_from_repo_flags(args)
    if not repos:
        print(
            f"{Fore.YELLOW}No repositories selected, specify --local-repo (-l) and/or --enable-repo (-r){Fore.RESET}",
            file=sys.stderr,
        )
        sys.exit(1)

    if args.recursive:
        return _list_recursively(prefix, end, repos, args)

    if not args.package:
        for repo_name, repo in repos.items():
            for name in repo.list_packages():
                print(prefix.format(repo=repo_name) + name, end=end)
            continue

    elif "/" not in args.package:
        for repo in repos.values():
            for version in repo.list_package_versions(args.package):
                print(version, end=end)
            continue

    else:
        for repo in repos.values():
            pkg = spk.api.parse_ident(args.package)
            for build in repo.list_package_builds(pkg):
                if not build.build or build.build.is_source():
                    print(spk.io.format_ident(build), end=end)
                    continue

                if args.verbose:
                    spec = repo.read_spec(build)
                    options = spec.resolve_all_options(spk.api.OptionMap({}))
                    print(
                        spk.io.format_ident(build),
                        spk.io.format_options(options),
                        end=end,
                    )
                else:
                    print(spk.io.format_ident(build), end=end)


def _list_recursively(
    prefix: str,
    end: str,
    repos: Dict[str, spk.storage.Repository],
    args: argparse.Namespace,
) -> None:

    for repo_name, repo in repos.items():
        if args.package:
            packages = [args.package]
        else:
            packages = repo.list_packages()
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
                    if not build.build or build.build.is_source():
                        print(spk.io.format_ident(build), end=end)
                        continue

                    if args.verbose:
                        spec = repo.read_spec(build)
                        options = spec.resolve_all_options(spk.api.OptionMap({}))
                        print(
                            spk.io.format_ident(build),
                            spk.io.format_options(options),
                            end=end,
                        )
                    else:
                        print(spk.io.format_ident(build), end=end)
