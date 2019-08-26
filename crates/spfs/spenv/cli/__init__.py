from typing import Sequence
import os
import sys
import subprocess
import argparse
import colorama
from colorama import Fore, Back, Style

colorama.init()

import spenv

config = spenv.Config()


def main(argv: Sequence[str]) -> int:

    try:
        args = parse_args(argv)
    except SystemExit as e:
        return e.code

    try:
        args.func(args)
    except Exception as e:
        if args.debug:
            raise
        print(repr(e), file=sys.stderr)
        return 1

    return 0


def _commit(args):
    """TODO: package or platform..."""

    runtime = spenv.active_runtime()
    repo = config.repository()

    if args.kind == "package":
        result = repo.commit_runtime(runtime)
    else:
        raise NotImplementedError("commit", args.kind)

    print(f"{Fore.GREEN}created: {Fore.RESET}{result.ref}")
    for tag in args.tags:
        repo.tag(result.ref, tag)
        print(f"{Fore.BLUE} tagged: {Fore.RESET}{tag}")

    return


def _status(args):
    """Inspect the status of the current workspace"""

    runtime = spenv.active_runtime()
    if runtime is None:
        print(f"{Fore.RED}No Active Runtime{Fore.RESET}")
    else:
        print(f"{Fore.GREEN}Active Runtime{Fore.RESET}")
        print(f" run: {runtime.rootdir}")
        print()


def _runtimes(args):

    repo = config.repository()
    runtimes = repo.runtimes.list_runtimes()
    for runtime in runtimes:
        print(runtime.ref)


def _enter(args):
    """Enter a configured shell environment."""

    setattr(args, "command", ("/bin/bash", "--norc"))
    return _run(args)


def _run(args):
    """Run a program in a configured environment."""

    proc = spenv.run(*args.cmd)
    proc.wait()

    sys.exit(proc.returncode)


def _shell(args):

    args.cmd = ["/bin/bash"] + args.cmd
    _run(args)


def _search(args):

    # TODO: more than just exact
    repo = config.repository()
    for term in args.terms:
        layer = repo.read_ref(term)
        print(repr(layer))


def _install(args):

    runtime = spenv.active_runtime()
    repo = config.repository()

    layers = []
    for ref in args.refs:
        layers.append(repo.read_ref(ref))

    for layer in layers:
        if isinstance(layer, spenv.storage.Package):
            runtime.append_package(layer)
        else:
            raise NotImplementedError("TODO: handle others")

    print(["spenv-remount", runtime.overlay_args])
    proc = subprocess.Popen(["spenv-remount", runtime.overlay_args])
    proc.wait()
    sys.exit(proc.returncode)


def parse_args(argv: Sequence[str]) -> argparse.Namespace:

    parser = argparse.ArgumentParser(prog=spenv.__name__, description=spenv.__doc__)
    parser.add_argument("--debug", "-d", action="store_true")

    sub_parsers = parser.add_subparsers(dest="command", required=True)

    status_cmd = sub_parsers.add_parser("status", help=_status.__doc__)
    status_cmd.set_defaults(func=_status)

    runtimes_cmd = sub_parsers.add_parser("runtimes", help=_runtimes.__doc__)
    runtimes_cmd.set_defaults(func=_runtimes)

    enter_cmd = sub_parsers.add_parser("enter", help=_enter.__doc__)
    enter_cmd.add_argument(
        "refs", metavar="REF", nargs="*", help="TODO: something good"
    )
    enter_cmd.set_defaults(func=_enter)

    run_cmd = sub_parsers.add_parser("run", help=_run.__doc__)
    run_cmd.add_argument("refs", metavar="REF", nargs="*", help="TODO: something good")
    run_cmd.add_argument("cmd", nargs=argparse.REMAINDER)
    run_cmd.set_defaults(func=_run)

    shell_cmd = sub_parsers.add_parser("shell", help=_shell.__doc__)
    shell_cmd.add_argument("cmd", nargs=argparse.REMAINDER)
    shell_cmd.set_defaults(func=_shell)

    commit_cmd = sub_parsers.add_parser("commit", help=_commit.__doc__)
    commit_cmd.add_argument("kind", choices=["package", "platform"], help="TODO: help")
    commit_cmd.add_argument(
        "--tag", "-t", dest="tags", action="append", help="TODO: help"
    )
    commit_cmd.set_defaults(func=_commit)

    install_cmd = sub_parsers.add_parser("install", help=_install.__doc__)
    install_cmd.add_argument("refs", metavar="REF", nargs="+", help="TODO: help")
    install_cmd.set_defaults(func=_install)

    search_cmd = sub_parsers.add_parser("search", help=_search.__doc__)
    search_cmd.add_argument("terms", metavar="TERM", nargs="+")
    search_cmd.set_defaults(func=_search)

    return parser.parse_args(argv)
