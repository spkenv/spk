import sys
import argparse

import spenv


def register(sub_parsers: argparse._SubParsersAction) -> None:

    run_cmd = sub_parsers.add_parser("run", help=_run.__doc__)
    run_cmd.add_argument("refs", metavar="REF", nargs="*", help="TODO: something good")
    run_cmd.add_argument("cmd", nargs=argparse.REMAINDER)
    run_cmd.set_defaults(func=_run)


def _run(args: argparse.Namespace) -> None:
    """Run a program in a configured environment."""

    proc = spenv.exec_in_new_runtime(*args.cmd)
    proc.wait()

    sys.exit(proc.returncode)
