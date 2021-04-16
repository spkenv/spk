# Copyright (c) 2021 Sony Pictures Imageworks, et al.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/imageworks/spk

from typing import Any, Union
import argparse

import structlog

import spk

from . import _flags

_LOGGER = structlog.get_logger("cli")
_VALID_STAGES = ("sources", "build", "install")


def register(
    sub_parsers: argparse._SubParsersAction, **parser_args: Any
) -> argparse.ArgumentParser:

    test_cmd = sub_parsers.add_parser("test", help=_test.__doc__, **parser_args)
    test_cmd.add_argument(
        "--here",
        action="store_true",
        help=(
            "Test in the current directory, instead of the source package "
            "(mostly relevant when testing source and build stages)"
        ),
    )
    test_cmd.add_argument(
        "packages",
        metavar="FILE|PKG[@STAGE] ...",
        nargs="*",
        default=[""],
        help=(
            "The package(s) to test. Can be a file name or <name>/<version> of an existing package. "
            f"If stage is given is should be one of: {', '.join(_VALID_STAGES)}"
        ),
    )
    _flags.add_repo_flags(test_cmd, default_local=True)
    _flags.add_option_flags(test_cmd)
    _flags.add_runtime_flags(test_cmd)
    test_cmd.set_defaults(func=_test)
    return test_cmd


def _test(args: argparse.Namespace) -> None:
    """Run package tests, to run install tests the package must have been built already."""

    runtime = _flags.ensure_active_runtime(args)

    options = _flags.get_options_from_flags(args)
    repos = _flags.get_repos_from_repo_flags(args)
    for package in args.packages:
        name, *stages = package.split("@", 1)
        stages = stages or _VALID_STAGES

        try:
            spec, filename = _flags.find_package_spec(name)
        except FileNotFoundError:
            filename = package
            pkg = spk.api.parse_ident(package)
            for repo in repos.values():
                try:
                    spec = repo.read_spec(pkg)
                    break
                except spk.storage.PackageNotFoundError:
                    continue
            else:
                raise spk.storage.PackageNotFoundError(package)

        for stage in stages:
            _LOGGER.info(f"Testing {filename}@{stage}...")

            tested = set()
            for variant in spec.build.variants:

                if not args.no_host:
                    opts = spk.api.host_options()
                else:
                    opts = spk.api.OptionMap()

                opts.update(variant)
                opts.update(options)
                digest = opts.digest()
                if digest in tested:
                    continue
                tested.add(digest)

                for index, test in enumerate(spec.tests):
                    if test.stage != stage:
                        continue

                    for selector in test.selectors:
                        selected_opts = opts.copy()
                        selected_opts.update(selector)
                        if selected_opts.digest() == digest:
                            break
                    else:
                        if test.selectors:
                            _LOGGER.info(
                                "SKIP: variant not selected", test=index, variant=opts
                            )
                            continue
                    _LOGGER.info("Running test", test=index, variant=opts)

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

                    tester = (
                        tester.with_options(opts)
                        .with_repositories(repos.values())
                        .with_requirements(test.requirements)
                    )
                    if args.here:
                        tester = tester.with_source(".")
                    try:
                        tester.test()
                    except spk.SolverError:
                        _LOGGER.error("failed to resolve test environment")
                        if args.verbose:
                            graph = tester.get_solve_graph()
                            print(
                                spk.io.format_solve_graph(graph, verbosity=args.verbose)
                            )
                        raise
