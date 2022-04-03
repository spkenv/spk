# Copyright (c) 2021 Sony Pictures Imageworks, et al.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/imageworks/spk

from typing import Any, Optional
import argparse
import sys

import structlog

import spk

from . import _flags

_LOGGER = structlog.get_logger("cli")


def register(
    sub_parsers: argparse._SubParsersAction, **parser_args: Any
) -> argparse.ArgumentParser:

    explain_cmd = sub_parsers.add_parser(
        "explain", help=_explain.__doc__, description=_explain.__doc__, **parser_args
    )
    _flags.add_solver_flags(explain_cmd)
    _flags.add_request_flags(explain_cmd)
    explain_cmd.add_argument(
        "packages",
        metavar="PKG",
        nargs="+",
        help="The initial set of packages to resolve",
    )
    explain_cmd.set_defaults(func=_explain)
    return explain_cmd


def _explain(args: argparse.Namespace) -> None:
    """Print the decision tree for the resolve of a set of packages."""

    solver = _flags.get_solver_from_flags(args)

    for request in _flags.parse_requests_using_flags(args, *args.packages):
        solver.add_request(request)

    solution: Optional[spk.Solution] = None
    err: Optional[Exception] = None
    try:
        generator = solver.run()
        spk.io.print_decisions(generator, args.verbose + 1)
        solution = generator.solution()
    except spk.SolverError as e:
        err = e

    if solution is not None:
        print("\n", spk.io.format_solution(solution, args.verbose))
    if err is not None:
        print(spk.io.format_error(err, args.verbose), file=sys.stderr)
        raise SystemExit(1)
