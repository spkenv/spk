from typing import Union
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
        item = repo.read_ref(ref)
        _pretty_print_ref(item)


def _pretty_print_ref(ref: Union[spenv.storage.Runtime, spenv.storage.Layer]) -> None:

    # TODO: use more format print/formatter types
    if isinstance(ref, spenv.storage.Platform):
        print(f"{Fore.YELLOW}PLATFORM{Fore.RESET}")
        print(f"{Fore.BLUE}root:{Fore.RESET} " + ref.rootdir)
        print(f"{Fore.BLUE}layers:{Fore.RESET}")
        for layer in ref.layers:
            print(f" - " + layer)
    elif isinstance(ref, spenv.storage.Package):
        print(f"{Fore.YELLOW}PACKAGE{Fore.RESET}")
        print(f"{Fore.BLUE}root:{Fore.RESET} " + ref.rootdir)
        print(f"{Fore.BLUE}manifest:{Fore.RESET} " + ref.config.manifest)
        print(f"{Fore.BLUE}envrion:{Fore.RESET}")
        for pair in ref.config.environ:
            print(" - " + pair)
    else:
        print(repr(ref))


def _print_global_info() -> None:
    """Display the status of the current runtime."""

    runtime = spenv.active_runtime()
    if runtime is None:
        print(f"{Fore.RED}No Active Runtime{Fore.RESET}")
        return

    print(f"{Fore.GREEN}Active Runtime:{Fore.RESET}")
    print(f" ref: {runtime.ref}")
    print()

    empty_manifest = spenv.tracking.Manifest(runtime.upperdir)
    manifest = spenv.tracking.compute_manifest(runtime.upperdir)
    diffs = spenv.tracking.compute_diff(empty_manifest, manifest)
    if len(diffs) == 1:
        print(f"{Fore.RED}No Active Changes{Fore.RESET}")
        return

    print(f"{Fore.BLUE}Active Changes:{Fore.RESET}")
    for diff in diffs:
        color = Fore.RESET
        if diff.mode == spenv.tracking.DiffMode.added:
            color = Fore.GREEN
        elif diff.mode == spenv.tracking.DiffMode.removed:
            color = Fore.RED
        elif diff.mode == spenv.tracking.DiffMode.changed:
            color = Fore.BLUE
        print(f"{color} {diff}{Fore.RESET}")
