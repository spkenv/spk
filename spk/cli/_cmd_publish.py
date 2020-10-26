from typing import Any
import argparse
import sys

import structlog
from colorama import Fore

import spk

_LOGGER = structlog.get_logger("spk.cli")


def register(
    sub_parsers: argparse._SubParsersAction, **parser_args: Any
) -> argparse.ArgumentParser:

    publish_cmd = sub_parsers.add_parser(
        "publish", help=_publish.__doc__, **parser_args
    )
    publish_cmd.add_argument(
        "packages", metavar="PKG", nargs="+", help="The local package to publish"
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

    publisher = (
        spk.Publisher()
        .with_target(args.target_repo)
        .force(args.force)
        .skip_source_packages(args.no_source)
    )

    for pkg in args.packages:

        publisher.publish(pkg)

    _LOGGER.info("done")
