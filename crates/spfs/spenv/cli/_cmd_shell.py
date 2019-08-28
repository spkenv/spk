import os
import sys
import argparse

from colorama import Fore
import structlog

import spenv

_logger = structlog.get_logger()


def register(sub_parsers: argparse._SubParsersAction) -> None:

    shell_cmd = sub_parsers.add_parser("shell", help=_shell.__doc__)
    shell_cmd.set_defaults(func=_shell)


def _shell(args: argparse.Namespace) -> None:

    print(f"Resolving spenv entry process...", end="", file=sys.stderr, flush=True)
    # TODO: resolve the shell more smartly
    exe = os.getenv("SHELL", "/bin/bash")
    cmd = spenv.build_command(exe)
    print(f"{Fore.GREEN}OK{Fore.RESET}", file=sys.stderr)
    os.execv(cmd[0], cmd)
