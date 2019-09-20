from typing import Sequence, Dict
import os
import sys
import logging
import argparse

import colorama
import structlog

import spenv
from . import (
    _cmd_commit,
    _cmd_info,
    _cmd_init,
    _cmd_install,
    _cmd_layers,
    _cmd_platforms,
    _cmd_run,
    _cmd_runtimes,
    _cmd_shell,
    _cmd_version,
)


def parse_args(argv: Sequence[str]) -> argparse.Namespace:

    parser = argparse.ArgumentParser(prog=spenv.__name__, description=spenv.__doc__)
    parser.add_argument("--debug", "-d", action="store_true")

    sub_parsers = parser.add_subparsers(dest="command", required=True)

    _cmd_version.register(sub_parsers)

    _cmd_runtimes.register(sub_parsers)
    _cmd_layers.register(sub_parsers)
    _cmd_platforms.register(sub_parsers)

    _cmd_run.register(sub_parsers)
    _cmd_shell.register(sub_parsers)

    _cmd_commit.register(sub_parsers)
    _cmd_install.register(sub_parsers)
    _cmd_info.register(sub_parsers)

    _cmd_init.register(sub_parsers)

    return parser.parse_args(argv)


def configure_logging(args: argparse.Namespace) -> None:

    colorama.init()
    level = logging.INFO
    processors = [
        structlog.stdlib.filter_by_level,
        structlog.stdlib.add_log_level,
        structlog.stdlib.PositionalArgumentsFormatter(),
    ]

    if args.debug:
        level = logging.DEBUG
        processors.extend(
            [
                structlog.stdlib.add_logger_name,
                structlog.processors.StackInfoRenderer(),
                structlog.processors.format_exc_info,
            ]
        )

    processors.append(structlog.dev.ConsoleRenderer())

    logging.basicConfig(stream=sys.stdout, format="%(message)s", level=level)
    structlog.configure(
        logger_factory=structlog.stdlib.LoggerFactory(),
        wrapper_class=structlog.stdlib.BoundLogger,
        processors=processors,
    )
