import sys
import shutil
import argparse

from colorama import Fore, Style

import spfs


def register(sub_parsers: argparse._SubParsersAction) -> None:

    ls_cmd = sub_parsers.add_parser(
        "ls-tags", aliases=["list-tags"], help=_ls_tags.__doc__
    )
    ls_cmd.add_argument(
        "path",
        metavar="PATH",
        nargs="?",
        default="/",
        help="The tag path to list under, defaults to the root ('/')",
    )
    ls_cmd.set_defaults(func=_ls_tags)


def _ls_tags(args: argparse.Namespace) -> None:
    """List tags by their path."""

    config = spfs.get_config()
    repo = config.get_repository()

    names = spfs.ls_tags(args.path)
    for name in names:
        print(name)
