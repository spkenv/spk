from typing import Sequence, Dict
import os
import sys
import logging
import getpass
import argparse

import colorama
import structlog
import sentry_sdk

import spenv
from . import (
    _cmd_commit,
    _cmd_info,
    _cmd_init,
    _cmd_layers,
    _cmd_platforms,
    _cmd_push,
    _cmd_pull,
    _cmd_run,
    _cmd_runtimes,
    _cmd_shell,
    _cmd_version,
)


def parse_args(argv: Sequence[str]) -> argparse.Namespace:

    parser = argparse.ArgumentParser(prog=spenv.__name__, description=spenv.__doc__)
    parser.add_argument("--debug", "-d", action="store_true")

    sub_parsers = parser.add_subparsers(dest="command", metavar="COMMAND")

    _cmd_version.register(sub_parsers)

    _cmd_runtimes.register(sub_parsers)
    _cmd_layers.register(sub_parsers)
    _cmd_platforms.register(sub_parsers)

    _cmd_run.register(sub_parsers)
    _cmd_shell.register(sub_parsers)

    _cmd_commit.register(sub_parsers)
    _cmd_push.register(sub_parsers)
    _cmd_pull.register(sub_parsers)
    _cmd_info.register(sub_parsers)

    _cmd_init.register(sub_parsers)

    args = parser.parse_args(argv)
    if args.command is None:
        parser.print_help(sys.stderr)
        sys.exit(1)
    return args


def configure_sentry() -> None:

    sentry_sdk.init("http://0dbf3ec96df2464ab626a50d0f352d44@sentry.spimageworks.com/5")
    with sentry_sdk.configure_scope() as scope:
        username = getpass.getuser()
        scope.user = {"email": f"{username}@imageworks.com", "username": username}


def configure_logging(args: argparse.Namespace) -> None:

    colorama.init()
    level = logging.INFO
    processors = [
        structlog.stdlib.filter_by_level,
        structlog.stdlib.add_log_level,
        structlog.stdlib.PositionalArgumentsFormatter(),
    ]

    if args.debug or "SPENV_DEBUG" in os.environ:
        os.environ["SPENV_DEBUG"] = "1"
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
