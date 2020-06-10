from typing import Any
import argparse

import structlog

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
    _flags.add_repo_flags(explain_cmd)
    _flags.add_option_flags(explain_cmd)
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

    options = _flags.get_options_from_flags(args)
    solver = spk.Solver(options)
    _flags.configure_solver_with_repo_flags(args, solver)

    for package in args.packages:
        solver.add_request(package)

    try:
        solver.solve()
    except spk.SolverError:
        pass

    print(spk.io.format_decision_tree(solver.decision_tree))
