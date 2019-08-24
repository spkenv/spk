from typing import Sequence
import os
import sys
import subprocess
import argparse
import colorama
from colorama import Fore, Back, Style

colorama.init()

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
    """Commit the current runtime fileset into a layer."""

    wksp = spenv.discover_workspace(".")
    runtime = spenv.active_runtime()
    if runtime is None:
        raise RuntimeError("No active runtime to commit")

    layer = wksp.commit(args.message)
    print(layer.ref)


def _status(args):
    """Inspect the status of the current workspace"""

    wksp = spenv.discover_workspace(".")

    print(f"{Fore.GREEN}Active Workspace{Fore.RESET}")
    print(f" root: {wksp.rootdir}")
    print(f" data: {wksp.dotspenvdir}")
    print()

    runtime = spenv.active_runtime()
    if runtime is None:
        print(f"{Fore.RED}No Active Runtime{Fore.RESET}")
    else:
        print(f"{Fore.GREEN}Active Runtime{Fore.RESET}")
        print(f" run: {runtime.rootdir}")
        print(f" mnt: {runtime.get_mount_path()}")
        print(f" env: {runtime.get_env_root()}")
        print()

    repo = wksp.read_meta_repo()
    if not repo:
        print(f"{Fore.RED}No Active Tracking{Fore.RESET}")
    else:
        print(f"{Fore.GREEN}Active Tracking{Fore.RESET}")
        print(f" head: {repo.git_dir}")


def _diff(args):

    wksp = spenv.discover_workspace(".")
    wksp.diff()  # TODO: something more clearly updating the index...
    repo = wksp.read_meta_repo()
    if not repo:
        raise RuntimeError("TODO: better error, no tracking")
    proc = subprocess.Popen(["git", "diff", "HEAD"], cwd=repo.working_tree_dir)
    proc.wait()
    sys.exit(proc.returncode)


def _checkout(args):
    """Configure the current workspace to use a specific environment.

    TODO: think of a better docstring
    """
    wksp = spenv.discover_workspace(".")
    wksp.checkout(args.tag)


def _track(args):

    tag = spenv.tracking.Tag.parse(args.tag)
    try:
        wksp = spenv.discover_workspace(".")
        config = wksp.config
    except spenv.NoWorkspaceError:
        config = spenv.Config()

    repos = config.repository_storage()
    if args.local:
        repo = repos.create_local_repository(tag.path)
    else:
        repo = repos.clone_repository(tag.path)

    print(repo)


def _shell(args):
    """Enter a workspace-configured shell environment."""

    wksp = spenv.discover_workspace(".")
    runtime = wksp.setup_runtime()
    env = runtime.compile_environment()

    print(f"{Fore.YELLOW}ENTERING SPENV SUBSHELL...{Style.RESET_ALL}")

    proc = subprocess.Popen(["/bin/bash", "--norc"], env=env)  # FIXME: other shells...
    proc.wait()

    print(f"{Fore.YELLOW}EXITING SPENV SUBSHELL...{Style.RESET_ALL}")
    sys.exit(proc.returncode)


def parse_args(argv: Sequence[str]) -> argparse.Namespace:

    parser = argparse.ArgumentParser(prog=spenv.__name__, description=spenv.__doc__)
    sub_parsers = parser.add_subparsers(dest="command", required=True)

    init_cmd = sub_parsers.add_parser("init", help=_init.__doc__)
    init_cmd.add_argument(
        "path",
        metavar="PATH",
        nargs="?",
        default=".",
        help="the location of the new workspace",
    )
    init_cmd.set_defaults(func=_init)

    track_cmd = sub_parsers.add_parser("track", help=_track.__doc__)
    track_cmd.add_argument("--local", action="store_true", help="TODO: betterify")
    track_cmd.add_argument("tag", metavar="TAG", help="tracking tag")
    track_cmd.set_defaults(func=_track)

    status_cmd = sub_parsers.add_parser("status", help=_status.__doc__)
    status_cmd.set_defaults(func=_status)

    status_cmd = sub_parsers.add_parser("diff", help=_diff.__doc__)
    status_cmd.set_defaults(func=_diff)

    checkout_cmd = sub_parsers.add_parser("checkout", help=_checkout.__doc__)
    checkout_cmd.add_argument("tag", metavar="TAG")
    checkout_cmd.set_defaults(func=_checkout)

    shell_cmd = sub_parsers.add_parser("shell", help=_shell.__doc__)
    shell_cmd.set_defaults(func=_shell)

    commit_cmd = sub_parsers.add_parser("commit", help=_commit.__doc__)
    commit_cmd.add_argument("--message", "-m", type=str)
    commit_cmd.set_defaults(func=_commit)

    return parser.parse_args(argv)
