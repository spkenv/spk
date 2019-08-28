import argparse

from colorama import Fore

import spenv


def register(sub_parsers: argparse._SubParsersAction) -> None:

    status_cmd = sub_parsers.add_parser("status", help=_status.__doc__)
    status_cmd.set_defaults(func=_status)


def _status(args: argparse.Namespace) -> None:
    """Display the status of the current runtime."""

    runtime = spenv.active_runtime()
    if runtime is None:
        print(f"{Fore.RED}No Active Runtime{Fore.RESET}")
        return

    print(f"{Fore.GREEN}Active Runtime{Fore.RESET}")
    print(f" run: {runtime.rootdir}")
    print()

    print(f"{Fore.BLUE}Active Changes:{Fore.RESET}")
    empty_manifest = spenv.tracking.Manifest(runtime.upperdir)
    manifest = spenv.tracking.compute_manifest(runtime.upperdir)
    diffs = spenv.tracking.compute_diff(empty_manifest, manifest)
    for diff in diffs:
        color = Fore.RESET
        if diff.mode == spenv.tracking.DiffMode.added:
            color = Fore.GREEN
        elif diff.mode == spenv.tracking.DiffMode.removed:
            colort = Fore.GREEN
        elif diff.mode == spenv.tracking.DiffMode.changed:
            color = Fore.BLUE
        print(f"{color} {diff}{Fore.RESET}")
