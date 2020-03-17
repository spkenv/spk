from typing import Sequence
import os
import sys
import socket
import logging
import getpass
import argparse

import spops
import colorama
import structlog
import sentry_sdk
from sentry_sdk.integrations.logging import ignore_logger

import spfs
from . import (
    _cmd_commit,
    _cmd_check,
    _cmd_clean,
    _cmd_diff,
    _cmd_edit,
    _cmd_info,
    _cmd_init,
    _cmd_layers,
    _cmd_log,
    _cmd_migrate,
    _cmd_platforms,
    _cmd_push,
    _cmd_pull,
    _cmd_reset,
    _cmd_run,
    _cmd_runtimes,
    _cmd_search,
    _cmd_shell,
    _cmd_tag,
    _cmd_tags,
    _cmd_version,
)


def parse_args(argv: Sequence[str]) -> argparse.Namespace:

    parser = argparse.ArgumentParser(prog=spfs.__name__, description=spfs.__doc__)
    parser.add_argument(
        "--debug", "-d", action="store_true", default=("SPFS_DEBUG" in os.environ)
    )

    sub_parsers = parser.add_subparsers(dest="command", metavar="COMMAND")

    _cmd_version.register(sub_parsers)

    _cmd_run.register(sub_parsers)
    _cmd_shell.register(sub_parsers)
    _cmd_edit.register(sub_parsers)
    _cmd_commit.register(sub_parsers)
    _cmd_reset.register(sub_parsers)

    _cmd_tag.register(sub_parsers)
    _cmd_push.register(sub_parsers)
    _cmd_pull.register(sub_parsers)

    _cmd_runtimes.register(sub_parsers)
    _cmd_layers.register(sub_parsers)
    _cmd_platforms.register(sub_parsers)
    _cmd_tags.register(sub_parsers)

    _cmd_info.register(sub_parsers)
    _cmd_log.register(sub_parsers)
    _cmd_search.register(sub_parsers)
    _cmd_diff.register(sub_parsers)

    _cmd_migrate.register(sub_parsers)
    _cmd_check.register(sub_parsers)
    _cmd_clean.register(sub_parsers)
    _cmd_init.register(sub_parsers)

    args = parser.parse_args(argv)
    if args.command is None:
        parser.print_help(sys.stderr)
        sys.exit(1)
    return args


def configure_sentry() -> None:

    sentry_sdk.init(
        "http://3dd72e3b4b9a4032947304fabf29966e@sentry.k8s.spimageworks.com/4",
        environment=os.getenv("SENTRY_ENVIRONMENT", "production"),
        release=spfs.__version__,
    )
    # the cli uses the logger after capturing errors explicitly,
    # so in this case we'll ask sentry to ignore all logging errors
    ignore_logger("cli")
    with sentry_sdk.configure_scope() as scope:
        username = getpass.getuser()
        scope.user = {"email": f"{username}@imageworks.com", "username": username}


def configure_spops() -> None:

    try:
        spops.configure(
            {
                "statsd": {"host": "statsd.k8s.spimageworks.com", "port": 30111},
                "labels": {
                    "environment": os.getenv("SENTRY_ENVIRONMENT", "production"),
                    "user": getpass.getuser(),
                    "host": socket.gethostname(),
                },
            },
        )
    except Exception as e:
        print(f"failed to initialize spops: {e}", file=sys.stderr)


def configure_logging(args: argparse.Namespace) -> None:

    colorama.init()
    level = logging.INFO
    processors = [
        structlog.stdlib.filter_by_level,
        structlog.stdlib.add_log_level,
        structlog.stdlib.PositionalArgumentsFormatter(),
    ]

    if args.debug:
        os.environ["SPFS_DEBUG"] = "1"
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
