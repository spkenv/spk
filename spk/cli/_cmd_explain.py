from typing import Any, Optional
import argparse

from ruamel import yaml
import structlog
from colorama import Fore

import spfs
import spk

from spk.io import format_decision

from . import _flags

_LOGGER = structlog.get_logger("cli")


def register(
    sub_parsers: argparse._SubParsersAction, **parser_args: Any
) -> argparse.ArgumentParser:

    explain_cmd = sub_parsers.add_parser(
        "explain", help=_explain.__doc__, **parser_args
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
        solution = solver.solve()
    except spk.SolverError as e:
        err = e

    print(spk.io.format_resolve(solver, args.verbose + 1))

    if solution is not None:
        print(spk.io.format_solution(solution, args.verbose))
    if err is not None:
        raise SystemExit(1)
