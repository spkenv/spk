from typing import Union
import os
import argparse

import structlog
from colorama import Fore

import spfs

_logger = structlog.get_logger("cli")


def register(sub_parsers: argparse._SubParsersAction) -> None:

    tag_cmd = sub_parsers.add_parser("tag", help=_tag.__doc__)
    tag_cmd.add_argument(
        "--remote",
        "-r",
        help=(
            "Create the tags in a remote repository instead of "
            "the local one (the target ref must exist in the remote repo)."
        ),
    )

    tag_cmd.add_argument("ref", metavar="TARGET_REF", nargs=1)
    tag_cmd.add_argument("tags", metavar="TAG", nargs="+")
    tag_cmd.set_defaults(func=_tag)


def _tag(args: argparse.Namespace) -> None:
    """Tag an object."""

    config = spfs.get_config()
    if args.remote is not None:
        repo = config.get_remote(args.remote)
    else:
        repo = config.get_repository()

    target = repo.read_ref(args.ref[0]).digest()
    for tag in args:
        repo.push_tag(tag, target)
        _logger.info("created", tag=tag)
