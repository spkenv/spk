from typing import Union
import os
import argparse

from colorama import Fore, Style

import spenv


def register(sub_parsers: argparse._SubParsersAction) -> None:

    info_cmd = sub_parsers.add_parser("info", help=_info.__doc__)
    info_cmd.add_argument(
        "--verbose", "-v", action="count", help="increase the verbosity of output"
    )
    info_cmd.add_argument("refs", metavar="REF", nargs="*")
    info_cmd.set_defaults(func=_info)


def _info(args: argparse.Namespace) -> None:
    """Display information about the current environment or specific items."""

    config = spenv.get_config()
    repo = config.get_repository()
    if not args.refs:
        _print_global_info()
        return
    for ref in args.refs:
        item = repo.read_object(ref)
        _pretty_print_ref(item, args.verbose)


def _pretty_print_ref(obj: spenv.storage.Object, verbosity: int = 0) -> None:

    if isinstance(obj, spenv.storage.Platform):
        print(f"{Fore.GREEN}platform:{Fore.RESET}")
        print(f" {Fore.BLUE}refs:{Fore.RESET} " + spenv.io.format_digest(obj.digest))
        print(f" {Fore.BLUE}stack:{Fore.RESET}")
        for ref in obj.stack:
            print(f"  - " + spenv.io.format_digest(ref))

    elif isinstance(obj, spenv.storage.Layer):
        print(f"{Fore.GREEN}layer:{Fore.RESET}")
        print(f" {Fore.BLUE}refs:{Fore.RESET} " + spenv.io.format_digest(obj.digest))
        print(f" {Fore.BLUE}manifest:{Fore.RESET}")
        if verbosity == 0:
            max_entries = 10
        if verbosity == 1:
            max_entries = 50
        else:
            max_entries = 0

        count = 0
        for path, entry in obj.manifest.walk():
            print(f"  {entry.mode:06o} {entry.kind.value} {path}")
            count += 1
            if max_entries and count == max_entries:
                print(f" {Style.DIM}  ...[truncated] use -vv for more{Style.RESET_ALL}")

    else:
        print(repr(obj))


def _print_global_info() -> None:
    """Display the status of the current runtime."""

    config = spenv.get_config()
    repo = config.get_repository()

    runtime = spenv.active_runtime()
    if runtime is None:
        print(f"{Fore.RED}No Active Runtime{Fore.RESET}")
        return

    print(f"{Fore.GREEN}Active Runtime:{Fore.RESET}")
    print(f" {Fore.BLUE}id:{Fore.RESET} {runtime.ref}")
    print(f" {Fore.BLUE}stack:{Fore.RESET}")
    stack = runtime.get_stack()
    for ref in stack:
        print(f"  - " + spenv.io.format_digest(ref))
    print()

    if not runtime.is_dirty():
        print(f"{Fore.RED}No Active Changes{Fore.RESET}")
        return

    print(f"{Fore.BLUE}Active Changes:{Fore.RESET}")
    base = spenv.compute_runtime_manifest(runtime)
    current = spenv.tracking.compute_manifest("/env")
    diffs = spenv.tracking.compute_diff(base, current)
    print(spenv.io.format_changes(diffs))
