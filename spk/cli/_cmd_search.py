from typing import Any
import argparse

import structlog

import spfs
import spk
from spk.io import format_request, format_ident

from . import _flags

_LOGGER = structlog.get_logger("cli")


def register(
    sub_parsers: argparse._SubParsersAction, **parser_args: Any
) -> argparse.ArgumentParser:

    search_cmd = sub_parsers.add_parser("search", help=_search.__doc__, **parser_args)
    _flags.add_repo_flags(search_cmd)
    search_cmd.add_argument("term", metavar="TERM", help="The search term / substring")
    search_cmd.set_defaults(func=_search)
    return search_cmd


def _search(args: argparse.Namespace) -> None:
    """Search for packages by substring."""

    repos = {}
    if args.local_repo:
        repos["local"] = spk.storage.local_repository()
    for name in args.enable_repo:
        repos[name] = spk.storage.remote_repository(name)

    width = max(map(len, repos.keys()))
    for repo_name, repo in repos.items():
        for name in repo.list_packages():
            if args.term in name:
                versions = list(
                    spk.api.Ident(name, spk.api.parse_version(v))
                    for v in repo.list_package_versions(name)
                )
                for v in versions:
                    print(
                        ("{: <" + str(width) + "}").format(repo_name), format_ident(v)
                    )
