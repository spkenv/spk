import argparse

from colorama import Fore

import spenv


def register(sub_parsers: argparse._Subsub_parserssAction) -> None:

    commit_cmd = sub_parsers.add_parser("commit", help=_commit.__doc__)
    commit_cmd.add_argument("kind", choices=["package", "platform"], help="TODO: help")
    commit_cmd.add_argument(
        "--tag", "-t", dest="tags", action="append", help="TODO: help"
    )
    commit_cmd.set_defaults(func=_commit)


def _commit(args: argparse.Namespace) -> None:
    """Commit the current runtime state to storage."""

    runtime = spenv.active_runtime()
    config = spenv.get_config()
    repo = config.get_repository()

    if args.kind == "package":
        result = repo.commit_package(runtime)
    else:
        raise NotImplementedError("commit", args.kind)

    print(f"{Fore.GREEN}created: {Fore.RESET}{result.ref}")
    for tag in args.tags:
        repo.tag(result.ref, tag)
        print(f"{Fore.BLUE} tagged: {Fore.RESET}{tag}")

    return
