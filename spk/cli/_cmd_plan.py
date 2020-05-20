from typing import Callable, Any
import argparse
import os
import sys

import spfs
import structlog
from colorama import Fore

import spk

_LOGGER = structlog.get_logger("spk.cli")


def register(sub_parsers: argparse._SubParsersAction) -> argparse.ArgumentParser:

    plan_cmd = sub_parsers.add_parser("plan", help=_plan.__doc__)
    plan_cmd.add_argument(
        "packages", metavar="PKG", nargs="+", help="The packages desired",
    )
    plan_cmd.set_defaults(func=_plan)
    return plan_cmd


def _plan(args: argparse.Namespace) -> None:
    """Build a package from its spec file."""

    solver = spk.Solver(spk.api.host_options())  # TODO: CLI
    for package in args.packages:

        if os.path.isfile(package):
            spec = spk.api.read_spec_file(package)
            solver.add_spec(spec)
        else:
            pkg = spk.api.parse_ident(package)
            solver.add_request(pkg)

    env = solver.solve()
    for path, node in spk.graph.walk_inputs_out(env):
        print(path, node)
