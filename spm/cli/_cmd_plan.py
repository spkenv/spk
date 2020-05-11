from typing import Callable, Any
import argparse
import os
import sys

import spfs
import structlog
from colorama import Fore

import spm

_LOGGER = structlog.get_logger("spm.cli")


def register(sub_parsers: argparse._SubParsersAction) -> argparse.ArgumentParser:

    plan_cmd = sub_parsers.add_parser("plan", help=_plan.__doc__)
    plan_cmd.add_argument(
        "packages", metavar="PKG", nargs="+", help="The packages desired",
    )
    plan_cmd.set_defaults(func=_plan)
    return plan_cmd


def _plan(args: argparse.Namespace) -> None:
    """Build a package from its spec file."""

    planner = spm.Planner()
    for package in args.packages:

        if os.path.isfile(package):
            spec = spm.api.read_spec_file(package)
            planner.add_spec(spec)
        else:
            pkg = spm.api.parse_ident(package)
            planner.add_package(pkg)

    plan = planner.plan()
    for output in plan.outputs():
        print(output)
        for path, node in spm.graph.walk_inputs_out(output):
            print(path, node)
