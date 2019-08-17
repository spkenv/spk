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
        print(e, file=sys.stderr)
        return 1

    return 0


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


def parse_args(argv: Sequence[str]) -> argparse.Namespace:

    parser = argparse.ArgumentParser(prog=spenv.__name__, description=spenv.__doc__)
    sub_parsers = parser.add_subparsers(dest="command", required=True)

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
