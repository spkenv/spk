from typing import Any
import argparse

import structlog

import spfs
import spk

from ._fmt import format_decision

_LOGGER = structlog.get_logger("cli")


def register(
    sub_parsers: argparse._SubParsersAction, **parser_args: Any
) -> argparse.ArgumentParser:

    explain_cmd = sub_parsers.add_parser(
        "explain", help=_explain.__doc__, **parser_args
    )
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

    options = spk.api.host_options()
    solver = spk.Solver(options)
    solver.add_repository(
        spk.storage.SpFSRepository(spfs.get_config().get_repository())  # FIXME: !!
    )
    for package in args.packages:
        solver.add_request(package)

    try:
        solver.solve()
    except spk.SolverError:
        pass

    for decision in solver.decision_tree.walk():

        print("." * decision.level(), format_decision(decision))
