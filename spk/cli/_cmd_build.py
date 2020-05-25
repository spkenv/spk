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

    build_cmd = sub_parsers.add_parser("build", help=_build.__doc__)
    build_cmd.add_argument(
        "--no-runtime",
        "-nr",
        action="store_true",
        help="Do not build in a new spfs runtime for debugging (will reset the current runtime)",
    )
    build_cmd.add_argument(
        "--here",
        action="store_true",
        help="Build from the current directory, instead of a source package",
    )
    build_cmd.add_argument(
        "packages",
        metavar="PKG|SPEC_FILE",
        nargs="+",
        help="The packages or yaml specification files to build",
    )
    build_cmd.set_defaults(func=_build)
    return build_cmd


def _build(args: argparse.Namespace) -> None:
    """Build a package from its spec file."""

    if not args.no_runtime:
        runtime = spfs.get_config().get_runtime_storage().create_runtime()
        runtime.set_editable(True)
        cmd = spfs.build_command_for_runtime(runtime, *sys.argv, "--no-runtime")
        os.execv(cmd[0], cmd)

    solver = spk.Solver(spk.api.host_options())
    solver.add_repository(spfs.get_config().get_repository())
    for package in args.packages:

        if os.path.isfile(package):
            spec = spk.api.read_spec_file(package)
            solver.add_spec(spec)
        else:
            pkg = spk.api.parse_ident(package)
            solver.add_request(pkg)

    env = solver.solve()
    spk.graph.execute_tree(env)
