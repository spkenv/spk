import os
import sys
import argparse

from colorama import Fore
import structlog

import spenv

_logger = structlog.get_logger()


def register(sub_parsers: argparse._SubParsersAction) -> None:

    run_cmd = sub_parsers.add_parser("run", help=_run.__doc__)
    run_cmd.add_argument(
        "target",
        metavar="REF",
        nargs=1,
        help="The runtime, platform or package to define the runtime environment",
    )
    run_cmd.add_argument("cmd", metavar="CMD", nargs=1)
    run_cmd.add_argument("args", metavar="ARGS", nargs=argparse.REMAINDER)
    run_cmd.set_defaults(func=_run)


def _run(args: argparse.Namespace) -> None:
    """Run a program in a configured environment."""

    # TODO: clean up this logic / break into function join with shell cmd logic
    config = spenv.get_config()
    repo = config.get_repository()
    target = repo.read_object(args.target[0])
    if isinstance(target, spenv.storage.fs.Runtime):
        runtime = target
    else:
        print(f"Configuring new runtime...", end="", file=sys.stderr, flush=True)
        runtime = repo.runtimes.create_runtime()
        spenv.install_to(runtime, args.target[0])
        print(f"{Fore.GREEN}OK{Fore.RESET}", file=sys.stderr)

    print(f"Resolving entry process...", end="", file=sys.stderr, flush=True)
    cmd = spenv.build_command_for_runtime(runtime, args.cmd[0], *args.args)
    print(f"{Fore.GREEN}OK{Fore.RESET}", file=sys.stderr)
    os.execv(cmd[0], cmd)
