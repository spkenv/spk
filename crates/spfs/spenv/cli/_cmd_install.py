import argparse

import structlog

import spenv

_logger = structlog.get_logger("cli")


def register(sub_parsers: argparse._SubParsersAction) -> None:

    install_cmd = sub_parsers.add_parser("install", help=_install.__doc__)
    install_cmd.add_argument("refs", metavar="REF", nargs="+", help="TODO: help")
    install_cmd.set_defaults(func=_install)


def _install(args: argparse.Namespace) -> None:

    spenv.install(*args.refs)
    # TODO: provide a way to source this, call init again?
    _logger.warning("environment requires update after install")
