from typing import Any, Union
import argparse
import os
import sys

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
        "--no-runtime",
        "-nr",
        action="store_true",
        help="Do not build in a new spfs runtime (useful for speed and debugging)",
    )
    test_cmd.add_argument(
        "packages",
        metavar="FILE|PKG[@STAGE] ...",
        nargs="*",
        default=[""],
        help="The package(s) to test",
    )
    _flags.add_repo_flags(test_cmd, default_local=True)
    test_cmd.set_defaults(func=_test)
    return test_cmd


_VALID_STAGES = ("sources", "build", "install")


def _test(args: argparse.Namespace) -> None:
    """Run package tests, to run install tests the package must have been built already."""

    if not args.no_runtime:
        runtime = spfs.get_config().get_runtime_storage().create_runtime()
        runtime.set_editable(True)
        cmd = spfs.build_command_for_runtime(runtime, *sys.argv, "--no-runtime")
        os.execv(cmd[0], cmd)
    else:
        runtime = spfs.active_runtime()

    repos = _flags.get_repos_from_repo_flags(args)
    for package in args.packages:
        name, *stages = package.split("@", 1)
        stages = stages or _VALID_STAGES

        spec, filename = _flags.find_package_spec(name)

        for stage in stages:

            _LOGGER.info(f"Testing {filename}@{stage}...")
            for test in spec.tests:
                if test.stage != stage:
                    continue

                tester: Union[
                    spk.test.PackageSourceTester,
                    spk.test.PackageBuildTester,
                    spk.test.PackageInstallTester,
                ]
                if stage == "sources":
                    tester = spk.test.PackageSourceTester(spec, test.script)
                elif stage == "build":
                    tester = spk.test.PackageBuildTester(spec, test.script)
                elif stage == "install":
                    tester = spk.test.PackageInstallTester(spec, test.script)
                else:
                    raise ValueError(
                        f"Untestable stage '{stage}', must be one of {_VALID_STAGES}"
                    )

                tester = tester.with_options(spk.api.host_options()).with_repositories(
                    repos.values()
                )
                try:
                    tester.test()
                except spk.SolverError:
                    _LOGGER.error("test failed")
                    if args.verbose:
                        tree = tester.get_test_env_decision_tree()
                        print(spk.io.format_decision_tree(tree, verbosity=args.verbose))
                    raise
