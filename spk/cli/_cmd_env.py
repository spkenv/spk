from typing import Any
import argparse
import os

import structlog

import spfs
import spk

from spk.io import format_decision

_LOGGER = structlog.get_logger("cli")


def register(
    sub_parsers: argparse._SubParsersAction, **parser_args: Any
) -> argparse.ArgumentParser:

    env_cmd = sub_parsers.add_parser("env", help=_env.__doc__, **parser_args)
    env_cmd.add_argument(
        "args",
        metavar="[PKG ...] -- [CMD] [ARGS ...]",
        nargs=argparse.REMAINDER,
        help="The packages and command to run",
    )
    env_cmd.set_defaults(func=_env)
    return env_cmd


def _env(args: argparse.Namespace) -> None:
    """Resolve and run an environment on the fly."""

    try:
        separator = args.args.index("--")
    except ValueError:
        separator = len(args.args)
    requests = args.args[:separator]
    command = args.args[separator + 1 :] or [os.getenv("SHELL", "/bin/bash")]

    options = spk.api.host_options()
    solver = spk.Solver(options)
    config = spfs.get_config()
    repo = spk.storage.SpFSRepository(config.get_repository())  # FIXME: !!
    solver.add_repository(repo)
    for request in requests:
        solver.add_request(request)

    packages = solver.solve()

    runtime = config.get_runtime_storage().create_runtime()
    for spec in packages.values():
        digest = repo.get_package(spec.pkg)
        runtime.push_digest(digest)

    cmd = spfs.build_command_for_runtime(runtime, *command)
    os.execvp(cmd[0], cmd)
