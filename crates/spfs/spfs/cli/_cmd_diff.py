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
        nargs=1,
        help="The tag or id to use as the base of the computed diff",
    )
    diff_cmd.add_argument(
        "top",
        metavar="TO",
        nargs="?",
        help="The tag or id to diff the base against, defaults to the current runtime",
    )
    diff_cmd.set_defaults(func=_diff)


def _diff(args: argparse.Namespace) -> None:
    """Compare two spfs file system states."""

    config = spfs.get_config()
    repo = config.get_repository()

    _logger.info("computing manifest", ref=args.base[0])
    base = spfs.compute_manifest(args.base[0])
    if args.top is None:
        rt = spfs.active_runtime()
        _logger.info("computing active runtime manifest")
        top = spfs.compute_runtime_manifest(rt)
    else:
        _logger.info("computing top manifest", ref=args.top)
        top = spfs.compute_manifest(args.top)

    _logger.info("computing diffs")
    diffs = spfs.tracking.compute_diff(base, top)
    print(spfs.io.format_changes(diffs))
