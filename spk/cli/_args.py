from typing import Sequence
import os
import sys
import getpass
import logging
import argparse

import spops
import sentry_sdk
import structlog
import colorama

import spk
from . import (
    _cmd_bake,
    _cmd_build,
    _cmd_convert,
    _cmd_deprecate,
    _cmd_env,
    _cmd_explain,
    _cmd_export,
    _cmd_import,
    _cmd_install,
    _cmd_ls,
    _cmd_make_binary,
    _cmd_make_source,
    _cmd_new,
    _cmd_publish,
    _cmd_remove,
    _cmd_search,
    _cmd_version,
    _cmd_view,
)


def parse_args(argv: Sequence[str]) -> argparse.Namespace:

    parent_parser = argparse.ArgumentParser(add_help=False)
    parent_parser.add_argument(
        "--verbose",
        "-v",
        action="count",
        help="Enable verbose output (can be specified more than once)",
        default=int(os.getenv("SPK_VERBOSITY", 0)),
    )

    parser = argparse.ArgumentParser(
        prog=spk.__name__, description=spk.__doc__, parents=[parent_parser]
    )

    sub_parsers = parser.add_subparsers(
        dest="command", title="commands", metavar="COMMAND"
    )

    _cmd_bake.register(sub_parsers, parents=[parent_parser])
    _cmd_build.register(sub_parsers, parents=[parent_parser])
    _cmd_convert.register(sub_parsers, parents=[parent_parser])
    _cmd_deprecate.register(sub_parsers, parents=[parent_parser])
    _cmd_env.register(sub_parsers, parents=[parent_parser])
    _cmd_explain.register(sub_parsers, parents=[parent_parser])
    _cmd_export.register(sub_parsers, parents=[parent_parser])
    _cmd_import.register(sub_parsers, parents=[parent_parser])
    _cmd_install.register(sub_parsers, parents=[parent_parser])
    _cmd_ls.register(sub_parsers, parents=[parent_parser])
    _cmd_make_binary.register(sub_parsers, parents=[parent_parser])
    _cmd_make_source.register(sub_parsers, parents=[parent_parser])
    _cmd_new.register(sub_parsers, parents=[parent_parser])
    _cmd_publish.register(sub_parsers, parents=[parent_parser])
    _cmd_remove.register(sub_parsers, parents=[parent_parser])
    _cmd_search.register(sub_parsers, parents=[parent_parser])
    _cmd_version.register(sub_parsers, parents=[parent_parser])
    _cmd_view.register(sub_parsers, parents=[parent_parser])

    args = parser.parse_args(argv)
    if args.command is None:
        parser.print_help(sys.stderr)
        sys.exit(1)
    return args


def configure_sentry() -> None:

    sentry_sdk.init(
        "http://4506b47108ac4b648fdf18a8d803f403@sentry.k8s.spimageworks.com/25",
        environment=os.getenv("SENTRY_ENVIRONMENT", "production"),
        release=spk.__version__,
    )
    with sentry_sdk.configure_scope() as scope:
        username = getpass.getuser()
        scope.user = {"email": f"{username}@imageworks.com", "username": username}


def configure_spops() -> None:

    try:
        spops.configure(
            {"statsd": {"host": "statsd.k8s.spimageworks.com", "port": 30111}}
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

    logging.getLogger("spfs").setLevel(logging.INFO)
    os.environ["SPK_VERBOSITY"] = str(args.verbose)
    if args.verbose > 0:
        level = logging.DEBUG
        processors.extend(
            [
                structlog.stdlib.add_logger_name,
                structlog.processors.StackInfoRenderer(),
                structlog.processors.format_exc_info,
            ]
        )
    if args.verbose > 1:
        logging.getLogger("spfs").setLevel(logging.INFO)
    if args.verbose > 2:
        logging.getLogger("spfs").setLevel(logging.DEBUG)

    processors.append(structlog.dev.ConsoleRenderer())

    logging.basicConfig(stream=sys.stderr, format="%(message)s", level=level)
    structlog.configure_once(
        logger_factory=structlog.stdlib.LoggerFactory(),
        wrapper_class=structlog.stdlib.BoundLogger,
        processors=processors,
    )
