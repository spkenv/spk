from typing import Union
import os
import argparse

from colorama import Fore

import spenv


def register(sub_parsers: argparse._SubParsersAction) -> None:

    info_cmd = sub_parsers.add_parser("info", help=_info.__doc__)
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
        _pretty_print_ref(item)


def _pretty_print_ref(obj: spenv.storage.Object) -> None:

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
        for path, entry in obj.manifest.walk():
            print(f"  {entry.mode:06o} {entry.kind.value} {path}")

    elif isinstance(obj, spenv.runtime.Runtime):
        print(f"{Fore.GREEN}runtime:{Fore.RESET}")
        print(f" {Fore.BLUE}refs:{Fore.RESET} " + spenv.io.format_digest(obj.digest))
        print(f" {Fore.BLUE}stack:{Fore.RESET}")
        for ref in obj.get_stack():
            print(f"  - " + spenv.io.format_digest(ref))
    else:
        print(repr(obj))


def _print_global_info() -> None:
    """Display the status of the current runtime."""

    runtime = spenv.active_runtime()
    if runtime is None:
        print(f"{Fore.RED}No Active Runtime{Fore.RESET}")
        return

    print(f"{Fore.GREEN}Active Runtime:{Fore.RESET}")
    print(f" ref: {runtime.ref}")
    print()

    empty_manifest = spenv.tracking.Manifest()
    manifest = spenv.tracking.compute_manifest(runtime.upper_dir)
    diffs = spenv.tracking.compute_diff(empty_manifest, manifest)
    if len(diffs) == 1:
        print(f"{Fore.RED}No Active Changes{Fore.RESET}")
        return

    print(f"{Fore.BLUE}Active Changes:{Fore.RESET}")
    print(spenv.io.format_diffs(diffs))
