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
    shell_cmd.add_argument("target", metavar="FILE|REF", nargs="?", help="TODO: help")
    shell_cmd.set_defaults(func=_shell)

    # This custom dict overrides the available subcommand
    # choices allowing spenv to be used as a shebang interpreter.
    # It does this by automatically selecting the "shell" subcommand
    # if given a valid path to a file
    shell_default_injector = _ShellCommandDefaultDict(sub_parsers.choices.items())
    sub_parsers.choices = shell_default_injector
    sub_parsers._name_parser_map = shell_default_injector


def _shell(args: argparse.Namespace) -> None:

    print(f"Resolving spenv entry process...", end="", file=sys.stderr, flush=True)

    # TODO: resolve the shell more smartly
    exe = os.getenv("SHELL", "/bin/bash")

    cmd_args: List[str] = []
    if args.command != "shell":
        # if the default shell command injection took place,
        # the actual command will be the path to the script
        # to execute, not 'shell'
        args.target = args.command

    if not args.target:
        cmd = spenv.build_command(exe, *cmd_args)

    else:
        # TODO: clean up this logic / break into function
        config = spenv.get_config()
        repo = config.get_repository()
        try:
            target = repo.read_ref(args.target)
            if isinstance(target, spenv.storage.Runtime):
                runtime = target
            else:
                runtime = repo.runtimes.create_runtime()
                spenv.install_to(runtime, args.target)
            cmd = spenv.build_command_for_runtime(runtime, exe, *cmd_args)
        except ValueError:
            cmd_args.append(args.target)
            cmd = spenv.build_command(exe, *cmd_args)

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
