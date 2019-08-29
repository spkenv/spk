from typing import Sequence, Dict
import os
import sys
import logging
import argparse

import colorama
import structlog

import spenv
from . import (
    _cmd_runtimes,
    _cmd_packages,
    _cmd_platforms,
    _cmd_run,
    _cmd_shell,
    _cmd_commit,
    _cmd_install,
    _cmd_init,
    _cmd_info,
)


def parse_args(argv: Sequence[str]) -> argparse.Namespace:

    parser = argparse.ArgumentParser(prog=spenv.__name__, description=spenv.__doc__)
    parser.add_argument("--debug", "-d", action="store_true")

    sub_parsers = parser.add_subparsers(dest="command", required=True)

    _cmd_runtimes.register(sub_parsers)
    _cmd_packages.register(sub_parsers)
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
        structlog.stdlib.add_log_level,
        structlog.stdlib.add_logger_name,
        structlog.stdlib.PositionalArgumentsFormatter(),
    ]

    if args.debug:
        level = logging.DEBUG
        processors.extend(
            [
                structlog.processors.StackInfoRenderer(),
                structlog.processors.format_exc_info,
            ]
        )

    processors.append(structlog.dev.ConsoleRenderer())

    structlog.configure(
        context_class=dict,
        logger_factory=structlog.stdlib.LoggerFactory(),
        wrapper_class=structlog.stdlib.BoundLogger,
        cache_logger_on_first_use=True,
    )

    root = logging.getLogger()
    root.setLevel(level)
    root.addHandler(logging.StreamHandler(sys.stderr))
