import argparse

import _cmd_run


def register(sub_parsers: argparse._SubParsersAction) -> None:

    shell_cmd = sub_parsers.add_parser("shell", help=_shell.__doc__)
    shell_cmd.add_argument("cmd", nargs=argparse.REMAINDER)
    shell_cmd.set_defaults(func=_shell)


def _shell(args: argparse.Namespace) -> None:

    args.cmd = ["/bin/bash"] + args.cmd
    # TODO: this is not a clear or clean dependency
    _cmd_run(args)
