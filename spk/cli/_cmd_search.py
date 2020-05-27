from typing import Any
import argparse

import structlog

import spfs
import spk

_LOGGER = structlog.get_logger("cli")


def register(
    sub_parsers: argparse._SubParsersAction, **parser_args: Any
) -> argparse.ArgumentParser:

    tags_cmd = sub_parsers.add_parser("search", help=_search.__doc__, **parser_args)
    tags_cmd.add_argument("term", metavar="TERM", help="The search term / substring")
    tags_cmd.set_defaults(func=_search)
    return tags_cmd


def _search(args: argparse.Namespace) -> None:
    """Search for packages by substring."""

    config = spfs.get_config()
    repos = []
    for name in config.list_remote_names():
        try:
            repos.append(spk.storage.SpFSRepository(config.get_remote(name)))
        except Exception as e:
            _LOGGER.warning("failed to open remote repository", remote=name)
            _LOGGER.warning("--> " + str(e))
    repos.insert(0, spk.storage.SpFSRepository(config.get_repository()))
    for repo in repos:
        for name in repo.list_packages():
            if args.term in name:
                print(f"{name} [{', '.join(repo.list_package_versions(name))}]")
