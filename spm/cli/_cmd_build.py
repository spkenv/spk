from typing import Callable, Any
import argparse
import os
import sys

import spfs
import structlog
from colorama import Fore

import spm

_LOGGER = structlog.get_logger("spm.cli")


def register(sub_parsers: argparse._SubParsersAction) -> argparse.ArgumentParser:

    build_cmd = sub_parsers.add_parser("build", help=_build.__doc__)
    build_cmd.add_argument(
        "--no-runtime",
        "-nr",
        action="store_true",
        help="Do not build in a new spfs runtime",
    )
    build_cmd.add_argument(
        "filename",
        metavar="SPEC_FILE",
        help="The yaml specification file for the package to build",
    )
    build_cmd.set_defaults(func=_build)
    return build_cmd


def _build(args: argparse.Namespace) -> None:
    """Build a package from its spec file."""

    if not args.no_runtime:
        runtime = spfs.get_config().get_runtime_storage().create_runtime()
        runtime.set_editable(True)
        cmd = spfs.build_command_for_runtime(runtime, *sys.argv, "--no-runtime")
        os.execv(cmd[0], cmd)

    spec = spm.read_spec_file(args.filename)
    os.chdir(os.path.dirname(args.filename))
    handles = spm.build_variants(spec)
    for handle in handles:
        _LOGGER.info(f"Created", pkg=str(handle.spec().pkg), url=handle.url())
