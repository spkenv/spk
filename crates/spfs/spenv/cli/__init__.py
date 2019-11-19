from typing import Sequence
import os
import sys
import subprocess
import argparse
import traceback

import logging
import structlog
import sentry_sdk

import spenv

from ._args import parse_args, configure_logging, configure_sentry

_logger = structlog.get_logger("cli")


def main() -> None:
    code = spenv.cli.run(sys.argv[1:])
    sys.exit(code)


def run(argv: Sequence[str]) -> int:

    configure_sentry()

    try:
        args = parse_args(argv)
    except SystemExit as e:
        return e.code

    configure_logging(args)

    with sentry_sdk.configure_scope() as scope:
        scope.set_extra("command", args.command)
        scope.set_extra("argv", sys.argv)

    try:
        args.func(args)

    except spenv.NoRuntimeError as e:
        _logger.error(str(e))
        return 1

    except Exception as e:
        sentry_sdk.capture_exception(e)
        _logger.error(str(e))
        if args.debug:
            traceback.print_exc(file=sys.stderr)
        return 1

    return 0
