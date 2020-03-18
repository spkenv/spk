import argparse

import spfs

from colorama import Fore, Style


def register(sub_parsers: argparse._SubParsersAction) -> None:

    log_cmd = sub_parsers.add_parser("log", help=_log.__doc__)
    log_cmd.add_argument(
        "--remote",
        "-r",
        help=(
            "Show the history from the given remote"
            " repository instead of the local storage"
        ),
    )
    log_cmd.add_argument("tag", metavar="TAG", help="The tag to show history of")
    log_cmd.set_defaults(func=_log)


def _log(args: argparse.Namespace) -> None:
    """Log the history of a given tag over time."""

    config = spfs.get_config()
    if args.remote is not None:
        repo = config.get_remote(args.remote)
    else:
        repo = config.get_repository()

    tag_stream = repo.tags.read_tag(args.tag)
    i = -1
    for tag in tag_stream:
        i += 1
        spec = spfs.tracking.build_tag_spec(tag.name, tag.org, i)
        spec_str = str(spec).ljust(len(tag.path) + 3)
        info = f"{Fore.YELLOW}{tag.target.str()[:10]}{Fore.RESET}"
        info += f" {Style.BRIGHT}{spec_str}{Style.RESET_ALL}"
        info += f" {Fore.BLUE}{tag.user}"
        info += f' {Fore.GREEN}{tag.time.strftime("%F %R")}{Fore.RESET}'
        print(info)
