from typing import Sequence
import os
import sys
import codecs
import subprocess
import argparse
import traceback
import logging

import spops
import sentry_sdk
from ruamel import yaml
from colorama import Fore

import spk

from ._args import parse_args, configure_logging, configure_sentry, configure_spops


def main() -> None:

    code = run(sys.argv[1:])
    sentry_sdk.flush()
    sys.exit(code)


def run(argv: Sequence[str]) -> int:

    try:
        configure_sentry()
    except Exception as e:
        print(f"failed to initialize sentry: {e}", file=sys.stderr)

    try:
        args = parse_args(argv)
    except SystemExit as e:
        return e.code

    configure_logging(args)
    configure_spops()

    spops.count("spk.run_count", command=args.command)

    with sentry_sdk.configure_scope() as scope:
        scope.set_extra("command", args.command)
        scope.set_extra("argv", sys.argv)

    try:
        sys.stdout.reconfigure(encoding="utf-8")  # type: ignore
    except AttributeError:
        # stdio might not be a real terminal, but that's okay
        pass

    try:

        with spops.timer("spk.run_time", command=args.command):
            args.func(args)

    except KeyboardInterrupt:
        pass

    except SystemExit as e:
        return e.code

    except Exception as e:
        _capture_if_relevant(e)
        spops.count("spk.error_count", command=args.command)
        print(f"{Fore.RED}{e}{Fore.RESET}", file=sys.stderr)
        if args.verbose > 1:
            print(f"{Fore.RED}{traceback.format_exc()}{Fore.RESET}", file=sys.stderr)
        return 1

    return 0


def _capture_if_relevant(e: Exception) -> None:

    sentry_sdk.capture_exception(e)
