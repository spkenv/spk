# Copyright (c) 2021 Sony Pictures Imageworks, et al.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/imageworks/spk

import os
from typing import Any
import argparse
import sys

import structlog
from colorama import Fore

import spk
import spkrs

from . import _flags

_LOGGER = structlog.get_logger("spk.cli")


def register(
    sub_parsers: argparse._SubParsersAction, **parser_args: Any
) -> argparse.ArgumentParser:

    render_cmd = sub_parsers.add_parser("render", help=_render.__doc__, **parser_args)
    render_cmd.add_argument(
        "packages", metavar="PKG", nargs="+", help="The packages to resolve and render"
    )
    render_cmd.add_argument(
        "target", metavar="PATH", help="The empty directory to render into"
    )
    _flags.add_request_flags(render_cmd)
    _flags.add_solver_flags(render_cmd)
    render_cmd.set_defaults(func=_render)
    return render_cmd


def _render(args: argparse.Namespace) -> None:
    """Output the contents of an spk environment (/spfs) to a folder."""

    solver = _flags.get_solver_from_flags(args)
    for name in args.packages:
        solver.add_request(name)

    for request in _flags.parse_requests_using_flags(args, *args.packages):
        solver.add_request(request)

    try:
        generator = solver.run()
        spk.io.format_decisions(generator, sys.stdout, args.verbose)
        solution = generator.solution
    except spk.SolverError as e:
        print(spk.io.format_error(e, args.verbose), file=sys.stderr)
        sys.exit(1)

    solution = spk.build_required_packages(solution)
    stack = spk.exec.resolve_runtime_layers(solution)
    path = os.path.abspath(args.target)
    os.makedirs(path, exist_ok=True)
    if len(os.listdir(path)) != 0:
        print(
            spk.io.format_error(
                ValueError(f"Directory is not empty {path}"), args.verbose
            ),
            file=sys.stderr,
        )
        sys.exit(1)
    _LOGGER.info(f"Rendering into dir: {path}")
    spkrs.render_into_dir(stack, path)
    _LOGGER.info(f"Render completed: {path}")
