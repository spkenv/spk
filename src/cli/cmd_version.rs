# Copyright (c) 2021 Sony Pictures Imageworks, et al.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/imageworks/spk

from typing import Any
import argparse

import spk


def register(
    sub_parsers: argparse._SubParsersAction, **parser_args: Any
) -> argparse.ArgumentParser:

    version_cmd = sub_parsers.add_parser(
        "version", help=_version.__doc__, description=_version.__doc__, **parser_args
    )
    version_cmd.set_defaults(func=_version)
    return version_cmd


def _version(_: argparse.Namespace) -> None:
    """Print the spk version number and exit."""

    print(spk.__version__)
