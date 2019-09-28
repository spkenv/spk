import argparse

import structlog

import spenv

from ._format import format_diffs

_logger = structlog.get_logger("cli")


def register(sub_parsers: argparse._SubParsersAction) -> None:

    install_cmd = sub_parsers.add_parser("install", help=_install.__doc__)
    install_cmd.add_argument("refs", metavar="REF", nargs="+", help="TODO: help")
    install_cmd.set_defaults(func=_install)


def _install(args: argparse.Namespace) -> None:

    original_startup_manifest = _get_startup_manifest()
    spenv.install(*args.refs)
    new_startup_manifest = _get_startup_manifest()
    diffs = spenv.tracking.compute_diff(original_startup_manifest, new_startup_manifest)
    if diffs:
        _logger.warning("installed targets made changes to startup files")
        _logger.warning("this means the current environment may need re-initializing")
        _logger.info("try 'source spenv startup'")
    if args.debug:
        _logger.debug(spenv.STARTUP_FILES_LOCATION)
        print(format_diffs(diffs))


def _get_startup_manifest() -> spenv.tracking.Manifest:

    try:
        return spenv.tracking.compute_manifest(spenv.STARTUP_FILES_LOCATION)
    except FileNotFoundError:
        return spenv.tracking.Manifest()
