# Copyright (c) 2021 Sony Pictures Imageworks, et al.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/imageworks/spk

from typing import Any
import subprocess
import re
import argparse

import structlog

from . import _flags

_LOGGER = structlog.get_logger("spk.cli")


def register(
    sub_parsers: argparse._SubParsersAction, **parser_args: Any
) -> argparse.ArgumentParser:

    convert_cmd = sub_parsers.add_parser(
        "convert", help=_convert.__doc__, description=_convert.__doc__, **parser_args
    )
    convert_cmd.add_argument("converter", nargs=1, help="the converter to run")
    convert_cmd.add_argument("args", nargs=argparse.REMAINDER)

    convert_cmd.set_defaults(func=_convert)
    return convert_cmd


def _convert(args: argparse.Namespace) -> None:
    """Convert a package from an external packaging system for use in spk."""

    converter = args.converter[0]
    exe = f"spk-convert-{converter}"
    cmd = [exe, *args.args]
    if converter == "pip":
        cmd = ["spk", "env", exe, "--", *cmd]
    subprocess.check_call(cmd)
