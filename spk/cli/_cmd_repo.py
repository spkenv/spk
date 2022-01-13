# Copyright (c) 2021 Sony Pictures Imageworks, et al.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/imageworks/spk

from typing import Any, List
import argparse
import sys

import structlog
from colorama import Fore, Style

import spkrs
import spk
import spk.io

from . import _flags

_LOGGER = structlog.get_logger("spk.cli")


def register(
    sub_parsers: argparse._SubParsersAction, **parser_args: Any
) -> argparse.ArgumentParser:

    repo_cmd = sub_parsers.add_parser(
        "repo", help=_repo.__doc__, description=_repo.__doc__, **parser_args
    )
    sub_parsers = repo_cmd.add_subparsers(dest="repo_command")
    upgrade_cmd = sub_parsers.add_parser(
        "upgrade", help=_upgrade.__doc__, **parser_args
    )
    upgrade_cmd.add_argument(
        "repo", metavar="REPO", nargs=1, help="The repository to upgrade"
    )
    upgrade_cmd.set_defaults(func=_upgrade)
    repo_cmd.set_defaults(func=_repo)
    return repo_cmd


def _repo(args: argparse.Namespace) -> None:
    """Perform repository-level actions and maintenance."""

    raise ValueError(
        f"subcommand is required and was not given: use 'spk repo --help' for more info"
    )


def _upgrade(args: argparse.Namespace) -> None:
    """Perform any pending upgrades to a package repository.

    This will bring the repository up-to-date for the current
    spk library version, but may also make it incompatible with
    older ones. Upgrades can also take time depending on their
    nature and the size of the repostory so. Please, take time to
    read any release and upgrade notes before invoking this.
    """

    repo = spk.storage.remote_repository(args.repo[0])
    print(repo.upgrade())
