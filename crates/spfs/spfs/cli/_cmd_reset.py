from typing import List
import sys
import argparse

import spfs

from colorama import Fore


def register(sub_parsers: argparse._SubParsersAction) -> None:

    reset_cmd = sub_parsers.add_parser("reset", help=_reset.__doc__)
    reset_cmd.add_argument(
        "--edit",
        "-e",
        action="store_true",
        help="mount the /spfs filesystem in edit mode (true if REF is empty or not given)",
    )
    reset_cmd.add_argument(
        "--ref",
        "-r",
        metavar="REF",
        help=(
            "The tag or id of the desired runtime, or the current runtime if not given."
            " Use '-' or an empty string to request an empty environment. Only valid"
            " if no paths are given"
        ),
    )
    reset_cmd.add_argument(
        "paths",
        metavar="PATH",
        nargs="*",
        help="Paths under /spfs to reset, or all paths if none given",
    )
    reset_cmd.set_defaults(func=_reset)


def _reset(args: argparse.Namespace) -> None:
    """Rebuild the current /spfs dir with the requested refs, removing any active changes."""

    config = spfs.get_config()
    repo = config.get_repository()
    runtime = spfs.active_runtime()
    if args.ref:

        if args.paths:
            print(
                f"{Fore.RED}Cannot specify both --ref and PATHs{Fore.RESET}",
                file=sys.stderr,
            )
            sys.exit(1)

        runtime.reset()
        runtime.reset_stack()
        if args.ref in ("-", ""):
            args.edit = True
        else:
            env_spec = spfs.tracking.EnvSpec(args.ref[0])
            for target in env_spec.tags:
                obj = repo.read_ref(target)
                runtime.push_digest(obj.digest())
    else:
        paths = _strip_spfs_prefix(args.paths)
        runtime.reset(*paths)

    if args.edit:
        runtime.set_editable(args.edit)

    spfs.remount_runtime(runtime)


def _strip_spfs_prefix(paths: List[str]) -> List[str]:
    out = []
    for path in paths:
        if path.startswith("/spfs"):
            path = path[len("/spfs") :]
        out.append(path)
    return out
