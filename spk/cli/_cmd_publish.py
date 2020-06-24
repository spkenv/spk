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

    publish_cmd = sub_parsers.add_parser(
        "publish", help=_publish.__doc__, **parser_args,
    )
    publish_cmd.add_argument(
        "packages", metavar="PKG", nargs="+", help="The local package to publish",
    )
    publish_cmd.add_argument(
        "--target-repo",
        "-r",
        type=str,
        metavar="NAME",
        default="origin",
        help="The repository to publish to. Any configured spfs repository can be named here.",
    )
    publish_cmd.add_argument(
        "--no-source",
        action="store_true",
        default=False,
        help="Skip publishing the related source package",
    )
    publish_cmd.add_argument(
        "--force",
        "-f",
        action="store_true",
        default=False,
        help="Forcefully overwrite any existing publishes",
    )
    publish_cmd.set_defaults(func=_publish)
    return publish_cmd


def _publish(args: argparse.Namespace) -> None:
    """publish a package into a shared repository."""

    local_repo = spk.storage.local_repository()
    remote_repo = spk.storage.remote_repository(args.target_repo)

    for p in args.packages:
        pkg = spk.api.parse_ident(p)
        if pkg.build is None:
            try:
                spec = local_repo.read_spec(pkg)
            except FileNotFoundError as e:
                print(f"{Fore.RED}{e}{Fore.RESET}", file=sys.stderr)
                sys.exit(1)

            try:
                _LOGGER.info("publishing spec", pkg=spec.pkg)
                if args.force:
                    remote_repo.force_publish_spec(spec)
                else:
                    remote_repo.publish_spec(spec)
            except spk.storage.VersionExistsError as e:
                print(f"{Fore.RED}{e}{Fore.RESET}", file=sys.stderr)
                sys.exit(1)
            builds = local_repo.list_package_builds(spec.pkg)
        else:
            builds = [pkg]

        for build in builds:

            if build == spk.api.SRC and args.no_source:
                _LOGGER.info("skipping source package (--no-source)")
                continue

            _LOGGER.info("publishing package", pkg=build)
            spec = local_repo.read_spec(build)
            digest = local_repo.get_package(build)
            spfs.sync_ref(
                str(digest), local_repo.as_spfs_repo(), remote_repo.as_spfs_repo()
            )
            remote_repo.publish_package(spec, digest)

    _LOGGER.info("done")
