from typing import Dict, List
import os
import sys
import argparse

from colorama import Fore
import structlog

import spenv

_logger = structlog.get_logger()


def register(sub_parsers: argparse._SubParsersAction) -> None:

    shell_cmd = sub_parsers.add_parser("shell", help=_shell.__doc__)
    shell_cmd.add_argument(
        "target",
        metavar="REF",
        nargs="?",
        help="The platform or layer to define the runtime environment",
    )
    shell_cmd.set_defaults(func=_shell)


def _shell(args: argparse.Namespace) -> None:

    # TODO: is there a better way to determine the shell to use?
    exe = os.getenv("SHELL", "/bin/bash")

    config = spenv.get_config()
    repo = config.get_repository()

    if args.target:
        try:
            repo.read_object(args.target)
        except ValueError:
            print(f"{args.target} does not exist locally, trying to pull")
            spenv.pull_ref(args.target)

    print(f"Configuring new runtime...", end="", file=sys.stderr, flush=True)
    runtime = repo.runtimes.create_runtime()
    if args.target:
        spenv.install_to(runtime, args.target)
    print(f"{Fore.GREEN}OK{Fore.RESET}", file=sys.stderr)

    print(f"Resolving entry process...", end="", file=sys.stderr, flush=True)
    cmd = spenv.build_command_for_runtime(runtime, exe)
    print(f"{Fore.GREEN}OK{Fore.RESET}", file=sys.stderr)
    os.execv(cmd[0], cmd)


class _ShellCommandDefaultDict(Dict[str, argparse.ArgumentParser]):
    """Automatically selects the shell command when only a script path is given.

    This dict replaces the argparse subcommand parser map, returning
    the shell command if the command is actually the path to a file
    on disk
    """

    def __contains__(self, name: object) -> bool:
        has_command = super(_ShellCommandDefaultDict, self).__contains__(name)
        if has_command:
            return True
        elif os.path.isfile(str(name)):
            return True
        return False

    def __getitem__(self, name: str) -> argparse.ArgumentParser:
        try:
            return super(_ShellCommandDefaultDict, self).__getitem__(name)
        except KeyError as e:
            if os.path.isfile(name):
                return self["shell"]
            raise
