from typing import Sequence
import os
import sys
import subprocess
import argparse

import spenv

REF_EMPTY = "empty"

SPENV_RUNTIME = "SPENV_RUNTIME"

ACTIVE_RUNTIME = os.getenv(SPENV_RUNTIME, None)


def main(argv: Sequence[str]) -> int:

    try:
        args = parse_args(argv)
    except SystemExit as e:
        return e.code

    try:
        args.func(args)
    except Exception as e:
        print(repr(e), file=sys.stderr)
        return 1

    return 0


def _init(args):
    """Initialize a spenv workspace."""

    wksp = spenv.create_workspace(args.path)
    print(f"initialized: {wksp.rootdir}")


def _commit(args):

    layer = spenv.commit(args.ref)
    print(f"created: {layer.ref}")


def _enter(args):

    repo = spenv.storage.configured_repository()

    if args.ref == REF_EMPTY:
        ref = repo.runtimes.create_runtime()
    else:
        ref = repo.read_ref(args.ref)
    if isinstance(ref, spenv.storage.Runtime):
        runtime = ref
    elif isinstance(ref, spenv.storage.Layer):
        runtime = repo.runtimes.create_runtime(args.ref)
    else:
        raise TypeError(ref)

    print(f"runtime: {runtime.ref}")

    mount_point = os.path.abspath(".spenv")
    os.makedirs(mount_point, exist_ok=True)

    print(f"mount: {mount_point}")
    spenv.mount(runtime.ref, mount_point)

    env = os.environ.copy()
    env["SPENV_BASE"] = args.ref
    env[SPENV_RUNTIME] = runtime.ref
    env["SPENV_MOUNT"] = mount_point
    print(f"ENTERING SUBSHELL for {args.ref}")
    proc = subprocess.Popen(
        [os.getenv("SHELL", "/bin/bash")], env=env  # FIXME: linux-only default
    )
    proc.wait()
    print(f"unmount: {mount_point}")
    spenv.unmount(mount_point)
    if args.rm:
        print(f"remove: {runtime.ref}")
        repo.runtimes.remove_runtime(runtime.ref)
    print(f"EXITING SUBSHELL for {args.ref}")
    sys.exit(proc.returncode)


def _runtimes(args):

    repo = spenv.storage.configured_repository()
    runtimes = repo.runtimes.list_runtimes()
    for runtime in runtimes:
        print(f"{runtime.ref}")


def _layers(args):

    repo = spenv.storage.configured_repository()
    layers = repo.layers.list_layers()
    for layer in layers:
        print(f"{layer.ref}")


def _status(args):
    """Inspect the status of the current workspace"""

    wksp = spenv.discover_workspace(".")

    print(f"root: {wksp.rootdir}")
    print(f"data: {wksp.dotspenvdir}")


def _checkout(args):
    """Configure the current workspace to use a specific environment.

    TODO: think of a better docstring
    """
    wksp = spenv.discover_workspace(".")
    wksp.checkout(args.tag)


def parse_args(argv: Sequence[str]) -> argparse.Namespace:

    parser = argparse.ArgumentParser(prog=spenv.__name__, description=spenv.__doc__)
    sub_parsers = parser.add_subparsers(dest="command", required=True)

    init_cmd = sub_parsers.add_parser("init", help=_init.__doc__)
    init_cmd.add_argument(
        "path",
        metavar="PATH",
        nargs="?",
        default=".",
        help="the location of the new work space",
    )
    init_cmd.set_defaults(func=_init)

    status_cmd = sub_parsers.add_parser("status", help=_status.__doc__)
    status_cmd.set_defaults(func=_status)

    checkout_cmd = sub_parsers.add_parser("checkout", help=_checkout.__doc__)
    checkout_cmd.add_argument("tag", metavar="TAG")
    checkout_cmd.set_defaults(func=_checkout)

    # ---- TODO: reevaluate semantics below

    enter_cmd = sub_parsers.add_parser("enter", help="enter an environment")
    enter_cmd.add_argument("ref", metavar="REF")
    enter_cmd.add_argument(
        "--rm", action="store_true", help="remove the runtime and any changes upon exit"
    )
    enter_cmd.set_defaults(func=_enter)

    runtimes_cmd = sub_parsers.add_parser("runtimes", help="manage stored runtimes")
    runtimes_cmd.set_defaults(func=_runtimes)

    layers_cmd = sub_parsers.add_parser("layers", help="manage stored layers")
    layers_cmd.set_defaults(func=_layers)

    commit_cmd = sub_parsers.add_parser("commit", help="commit a runtime into a layer")
    commit_cmd.add_argument(
        "ref", metavar="REF", nargs="?", type=str, default=ACTIVE_RUNTIME
    )
    commit_cmd.set_defaults(func=_commit)

    return parser.parse_args(argv)
