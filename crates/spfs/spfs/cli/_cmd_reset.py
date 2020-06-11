import argparse
from datetime import datetime, timedelta

import spfs

from colorama import Fore, Style


def register(sub_parsers: argparse._SubParsersAction) -> None:

    reset_cmd = sub_parsers.add_parser("reset", help=_reset.__doc__)
    reset_cmd.add_argument(
        "--edit",
        "-e",
        action="store_true",
        help="mount the /spfs filesystem in edit mode (true if REF is empty or not given)",
    )
    reset_cmd.add_argument(
        "ref",
        metavar="REF",
        nargs=1,
        help="The tag or id of the desired runtime, "
        "use '-' or an empty string to request an empty environment",
    )
    reset_cmd.set_defaults(func=_reset)


def _reset(args: argparse.Namespace) -> None:
    """Rebuild the current /spfs dir with the requested refs, removing any active changes."""

    config = spfs.get_config()
    repo = config.get_repository()
    runtime = spfs.active_runtime()
    runtime.reset()
    if args.ref and args.ref[0] not in ("-", ""):
        env_spec = spfs.tracking.EnvSpec(args.ref[0])
        for target in env_spec.tags:
            obj = repo.read_ref(target)
            runtime.push_digest(obj.digest())
    else:
        args.edit = True

    runtime.set_editable(args.edit)
    spfs.remount_runtime(runtime)
