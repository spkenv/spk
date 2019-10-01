from typing import Sequence
import os
import sys
import subprocess
import argparse
import traceback

import logging
import structlog

import spenv

from ._args import parse_args, configure_logging

_logger = structlog.get_logger("cli")


def main() -> None:
    code = spenv.cli.run(sys.argv[1:])
    sys.exit(code)


def run(argv: Sequence[str]) -> int:

    try:
        args = parse_args(argv)
    except SystemExit as e:
        return e.code

    configure_logging(args)

    try:
        args.func(args)

    except spenv.NoRuntimeError as e:
        _logger.error(str(e))
        return 1

    except Exception as e:
        _logger.error(str(e))
        if args.debug:
            traceback.print_exc(file=sys.stderr)
        return 1

    return 0
