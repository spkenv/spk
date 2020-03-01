from typing import Union
import os
import argparse

import structlog
from colorama import Fore

import spfs

_logger = structlog.get_logger("cli")


def register(sub_parsers: argparse._SubParsersAction) -> None:

    tag_cmd = sub_parsers.add_parser("tag", help=_tag.__doc__)
    tag_cmd.add_argument("ref", metavar="REF", nargs=1)
    tag_cmd.add_argument("tags", metavar="TAG", nargs="+")
    tag_cmd.set_defaults(func=_tag)


def _tag(args: argparse.Namespace) -> None:
    """Tag an object."""

    config = spfs.get_config()
    repo = config.get_repository()
    target = args.ref[0]
    for tag in args.tags:
        repo.tags.push_tag(tag, target)
        _logger.info("created", tag=tag)
