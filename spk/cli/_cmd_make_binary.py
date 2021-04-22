# Copyright (c) 2021 Sony Pictures Imageworks, et al.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/imageworks/spk

from typing import Any
import argparse
import os

import structlog

import spk
from . import _flags

_LOGGER = structlog.get_logger("spk.cli")


def register(
    sub_parsers: argparse._SubParsersAction, **parser_args: Any
) -> argparse.ArgumentParser:

    mkb_cmd = sub_parsers.add_parser(
        "make-binary",
        aliases=["mkbinary", "mkbin", "mkb"],
        help=_make_binary.__doc__,
        description=_make_binary.__doc__,
        **parser_args
    )
    mkb_cmd.add_argument(
        "--here",
        action="store_true",
        help=("Build from the current directory, instead of a source package"),
    )
    mkb_cmd.add_argument(
        "--interactive",
        "-i",
        action="store_true",
        default=False,
        help="Setup the build, but instead of running the build script start an interactive shell",
    )
    mkb_cmd.add_argument(
        "--env",
        "-e",
        action="store_true",
        help="Build the first variant of this package, and then immediately enter a shell environment with it",
    )
    mkb_cmd.add_argument(
        "packages",
        metavar="PKG|SPEC_FILE",
        nargs="+",
        help="The packages or yaml specification files to build",
    )
    _flags.add_repo_flags(mkb_cmd)
    _flags.add_option_flags(mkb_cmd)
    _flags.add_runtime_flags(mkb_cmd)
    mkb_cmd.set_defaults(func=_make_binary)
    return mkb_cmd


def _make_binary(args: argparse.Namespace) -> None:
    """Build a binary package from a spec file or source package."""

    runtime = _flags.ensure_active_runtime(args)

    options = _flags.get_options_from_flags(args)
    repos = _flags.get_repos_from_repo_flags(args).values()
    for package in args.packages:
        if os.path.isfile(package):
            spec = spk.api.read_spec_file(package)
            _LOGGER.info("saving spec file", pkg=spec.pkg)
            spk.save_spec(spec)
        else:
            spec = spk.load_spec(package)

        _LOGGER.info("building binary package", pkg=spec.pkg)
        built = set()
        for variant in spec.build.variants:

            if not args.no_host:
                opts = spk.api.host_options()
            else:
                opts = spk.api.OptionMap()

            opts.update(variant)
            opts.update(options)
            if opts.digest() in built:
                continue
            built.add(opts.digest())

            _LOGGER.info("building variant", variant=opts)
            builder = (
                spk.BinaryPackageBuilder.from_spec(spec)
                .with_options(opts)
                .with_repositories(repos)
            )
            if args.here:
                builder = builder.with_source(os.getcwd())
            builder.set_interactive(args.interactive)
            try:
                out = builder.build()
            except (ValueError, spk.SolverError):
                _LOGGER.error("build failed", variant=opts)
                if args.verbose:
                    graph = builder.get_solve_graph()
                    print(spk.io.format_solve_graph(graph, verbosity=args.verbose))
                raise
            else:
                _LOGGER.info("created", pkg=out.pkg)
            if args.env:
                cmd = ["spk", "env", "-l", str(out.pkg)]
                _LOGGER.info("entering environment of new package", cmd=" ".join(cmd))
                os.execvp(cmd[0], cmd)
