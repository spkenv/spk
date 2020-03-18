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

    config = spfs.get_config()
    repo = config.get_repository()

    if args.base is None:
        _logger.debug("computing runtime manifest as base")
        rt = spfs.active_runtime()
        base = spfs.compute_runtime_manifest(rt)
    else:
        _logger.debug("computing base manifest", ref=args.base)
        base = spfs.compute_manifest(args.base)

    if args.top is None:
        _logger.debug("computing manifest for /spfs")
        top = spfs.tracking.compute_manifest("/spfs")
    else:
        _logger.debug("computing top manifest", ref=args.top)
        top = spfs.compute_manifest(args.top)

    _logger.debug("computing diffs")
    diffs = spfs.tracking.compute_diff(base, top)
    out = spfs.io.format_changes(diffs)
    if not out.strip():
        _logger.info("no changes")
    else:
        print(out)
