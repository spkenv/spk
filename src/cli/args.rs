# Copyright (c) 2021 Sony Pictures Imageworks, et al.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/imageworks/spk

from typing import Sequence
import os
import sys
import getpass
import logging
import argparse

import sentry_sdk
import structlog
import colorama

try:
    import spops
except ImportError:
    spops = None

import spk
import spkrs

def configure_sentry() -> None:

    from sentry_sdk.integrations.stdlib import StdlibIntegration
    from sentry_sdk.integrations.excepthook import ExcepthookIntegration
    from sentry_sdk.integrations.dedupe import DedupeIntegration
    from sentry_sdk.integrations.atexit import AtexitIntegration
    from sentry_sdk.integrations.logging import LoggingIntegration
    from sentry_sdk.integrations.argv import ArgvIntegration
    from sentry_sdk.integrations.modules import ModulesIntegration
    from sentry_sdk.integrations.threading import ThreadingIntegration

    sentry_sdk.init(
        "http://4506b47108ac4b648fdf18a8d803f403@sentry.k8s.spimageworks.com/25",
        environment=os.getenv("SENTRY_ENVIRONMENT", "production"),
        release=spk.__version__,
        default_integrations=False,
        integrations=[
            StdlibIntegration(),
            ExcepthookIntegration(),
            DedupeIntegration(),
            AtexitIntegration(),
            LoggingIntegration(),
            ArgvIntegration(),
            ModulesIntegration(),
            ThreadingIntegration(),
        ],
    )
    with sentry_sdk.configure_scope() as scope:
        username = getpass.getuser()
        scope.user = {"email": f"{username}@imageworks.com", "username": username}


def configure_spops() -> None:

    if spops is None:
        return
    try:
        spops.configure(
            {"statsd": {"host": "statsd.k8s.spimageworks.com", "port": 30111}}
        )
    except Exception as e:
        print(f"failed to initialize spops: {e}", file=sys.stderr)


def configure_logging(args: argparse.Namespace) -> None:

    colorama.init()
    spkrs.configure_logging(args.verbose)

    level = logging.INFO
    processors = [
        structlog.stdlib.filter_by_level,
        structlog.stdlib.add_log_level,
        structlog.stdlib.PositionalArgumentsFormatter(),
    ]

    logging.getLogger("spfs").setLevel(logging.INFO)
    logging.getLogger("urllib3").setLevel(logging.CRITICAL + 1)
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
