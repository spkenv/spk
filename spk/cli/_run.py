# Copyright (c) 2021 Sony Pictures Imageworks, et al.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/imageworks/spk

from typing import Sequence
import sys
import traceback

import sentry_sdk
from colorama import Fore

import spk

from ._args import parse_args, configure_logging, configure_sentry, configure_spops, spops


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

    if spops is not None:
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

        if spops is not None:
            with spops.timer("spk.run_time", command=args.command):
                args.func(args)
        else:
            args.func(args)

    except KeyboardInterrupt:
        pass

    except SystemExit as e:
        return e.code

    except Exception as e:
        _capture_if_relevant(e)
        if spops is not None:
            spops.count("spk.error_count", command=args.command)
        print(f"{spk.io.format_error(e)}", file=sys.stderr)
        if args.verbose > 2:
            print(f"{Fore.RED}{traceback.format_exc()}{Fore.RESET}", file=sys.stderr)
        return 1

    return 0


def _capture_if_relevant(err: Exception) -> None:

    if isinstance(err, spk.storage.PackageNotFoundError):
        return
    if isinstance(err, spk.storage.VersionExistsError):
        return
    if isinstance(err, spk.NoEnvironmentError):
        return
    if isinstance(err, spk.build.BuildError):
        return
    if isinstance(err, spk.solve.SolverError):
        return
    if isinstance(
        err,
        (
            spk.api.InvalidNameError,
            spk.api.InvalidVersionError,
            spk.api.InvalidBuildError,
        ),
    ):
        return
    sentry_sdk.capture_exception(err)
