from typing import Any, ValuesView
import argparse
import os
import subprocess

import structlog

import spfs
import spk

from spk.io import format_decision
from . import _flags

_LOGGER = structlog.get_logger("cli")


def register(
    sub_parsers: argparse._SubParsersAction, **parser_args: Any
) -> argparse.ArgumentParser:

    test_cmd = sub_parsers.add_parser("test", help=_test.__doc__, **parser_args)
    test_cmd.add_argument(
        "packages",
        metavar="FILE|PKG[@STAGE] ...",
        nargs="*",
        default=[""],
        help="The package(s) to test",
    )
    _flags.add_repo_flags(test_cmd)
    test_cmd.set_defaults(func=_test)
    return test_cmd


_VALID_STAGES = ("sources", "build", "install")


def _test(args: argparse.Namespace) -> None:
    """Run package tests, building as needed."""

    for package in args.packages:
        name, *stages = package.split("@", 1)
        stages = stages or _VALID_STAGES

        spec, filename = _flags.find_package_spec(name)

        for stage in stages:

            _LOGGER.info(f"Testing {filename}@{stage}...")

            if stage == "sources":
                tester = spk.test.PackageSourceTester(spec)
            elif stage == "build":
                tester = spk.test.PackageBuildTester(spec)
            elif stage == "install":
                tester = spk.test.PackageInstallTester(spec)
            else:
                raise ValueError(
                    f"Untestable stage '{stage}', must be one of {_VALID_STAGES}"
                )

            tester.test()
