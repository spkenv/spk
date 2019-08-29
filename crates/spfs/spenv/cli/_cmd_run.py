import os
import sys
import argparse

from colorama import Fore
import structlog

import spenv

_logger = structlog.get_logger()


def register(sub_parsers: argparse._SubParsersAction) -> None:

    run_cmd = sub_parsers.add_parser("run", help=_run.__doc__)
    # TODO: this does not works as it's ambiguous with remainder
    # run_cmd.add_argument("refs", metavar="REF", nargs="*", help="TODO: something good")
    run_cmd.add_argument("cmd", metavar="CMD", nargs=1)
    run_cmd.add_argument("args", metavar="ARGS", nargs=argparse.REMAINDER)
    run_cmd.set_defaults(func=_run)


def _run(args: argparse.Namespace) -> None:
    """Run a program in a configured environment."""

    print(f"Resolving spenv entry process...", end="", file=sys.stderr, flush=True)
    cmd = spenv.build_command(args.cmd, *args.args)
    print(f"{Fore.GREEN}OK{Fore.RESET}", file=sys.stderr)
    os.execv(cmd[0], cmd)
