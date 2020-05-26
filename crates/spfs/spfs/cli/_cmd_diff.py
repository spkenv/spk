import os
import argparse

import structlog

import spfs

_logger = structlog.get_logger("cli")


def register(sub_parsers: argparse._SubParsersAction) -> None:

    diff_cmd = sub_parsers.add_parser("diff", help=_diff.__doc__)
    diff_cmd.add_argument(
        "base",
        metavar="FROM",
        nargs="?",
        help="The tag or id to use as the base of the computed diff, defaults to the current runtime",
    )
    diff_cmd.add_argument(
        "top",
        metavar="TO",
        nargs="?",
        help="The tag or id to diff the base against, defaults to the contents of /spfs",
    )
    diff_cmd.set_defaults(func=_diff)


def _diff(args: argparse.Namespace) -> None:
    """Compare two spfs file system states."""

    diffs = spfs.diff(args.base, args.top)
    out = spfs.io.format_changes(diffs)
    if not out.strip():
        _logger.info("no changes")
    else:
        print(out)
