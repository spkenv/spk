from typing import Any
import argparse
import os

from colorama import Fore, Style
import structlog

import spfs
import spk

from spk.io import format_decision

from . import _flags

_LOGGER = structlog.get_logger("cli")


def register(
    sub_parsers: argparse._SubParsersAction, **parser_args: Any
) -> argparse.ArgumentParser:

    env_cmd = sub_parsers.add_parser("env", help=_env.__doc__, **parser_args)
    env_cmd.add_argument(
        "args",
        metavar="[PKG...] -- [CMD]",
        nargs=argparse.REMAINDER,
        help=(
            "The packages and optional command to run, "
            "use '--' to separate packages from command or if no command is given "
            "spawn a new shell"
        ),
    )
    _flags.add_repo_flags(env_cmd)
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
    _flags.configure_solver_with_repo_flags(args, solver)

    for request in requests:
        solver.add_request(request)

    try:
        packages = solver.solve()
    except spk.SolverError as e:
        print(f"{Fore.RED}{e}{Fore.RESET}")
        if args.verbose:
            print(spk.io.format_decision_tree(solver.decision_tree))
        else:
            print(f"{Fore.YELLOW}{Style.DIM}try '--verbose' for more info{Fore.RESET}")
        exit(1)

    runtime = spfs.get_config().get_runtime_storage().create_runtime()
    for spec in packages.values():
        for repo in _flags.get_repos_from_repo_flags(args).values():
            try:
                digest = repo.get_package(spec.pkg)
                runtime.push_digest(digest)
                break
            except FileNotFoundError:
                pass
        else:
            raise RuntimeError("Resolved package disspeared, please try again")

    os.environ["PATH"] = "/spfs/bin" + os.pathsep + os.getenv("PATH", "")
    os.environ["LD_LIBRARY_PATH"] = (
        "/spfs/lib" + os.pathsep + os.getenv("LD_LIBRARY_PATH", "")
    )
    os.environ["LIBRARY_PATH"] = (
        "/spfs/lib" + os.pathsep + os.getenv("LIBRARY_PATH", "")
    )

    cmd = spfs.build_command_for_runtime(runtime, *command)
    os.execvp(cmd[0], cmd)
