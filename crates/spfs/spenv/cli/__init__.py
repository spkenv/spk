from typing import Sequence
import os
import sys
import subprocess
import argparse
import traceback

import logging
import structlog
import colorama
from colorama import Fore, Back, Style

import spenv

from ._args import parse_args, configure_logging

colorama.init()
config = spenv.get_config()


def main() -> None:
    code = spenv.cli.run(sys.argv[1:])
    sys.exit(code)


def run(argv: Sequence[str]) -> int:

    try:
        args = parse_args(argv)
    except SystemExit as e:
        return e.code

    configure_logging(args)

    try:
        args.func(args)

    except spenv.NoRuntimeError as e:
        print(f"{Fore.RED}{e}{Fore.RESET}")
        return 1

    except Exception as e:
        print(f"{Fore.RED}{repr(e)}{Fore.RESET}", file=sys.stderr)
        if args.debug:
            print(f"{Fore.YELLOW}{traceback.format_exc()}{Fore.RESET}")
        return 1

    return 0
