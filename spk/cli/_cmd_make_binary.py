from typing import Callable, Any
import argparse
import os
import sys

import spfs
import structlog
from colorama import Fore

import spk

_LOGGER = structlog.get_logger("spk.cli")


def register(sub_parsers: argparse._SubParsersAction) -> argparse.ArgumentParser:

    mkb_cmd = sub_parsers.add_parser(
        "make-binary", aliases=["mkbinary", "mkbin", "mkb"], help=_make_binary.__doc__
    )
    mkb_cmd.add_argument(
        "--no-runtime",
        "-nr",
        action="store_true",
        help="Do not build in a new spfs runtime (useful for speed and debugging)",
    )
    mkb_cmd.add_argument(
        "--here",
        action="store_true",
        help=(
            "Build from the current directory, instead of a source package "
            "(only relevant when building from a source package, not yaml spec files)"
        ),
    )
    mkb_cmd.add_argument(
        "packages",
        metavar="PKG|SPEC_FILE",
        nargs="+",
        help="The packages or yaml specification files to build",
    )
    mkb_cmd.set_defaults(func=_make_binary)
    return mkb_cmd


def _make_binary(args: argparse.Namespace) -> None:
    """Build a binary package from a spec file or source package."""

    if not args.no_runtime:
        runtime = spfs.get_config().get_runtime_storage().create_runtime()
        runtime.set_editable(True)
        cmd = spfs.build_command_for_runtime(runtime, *sys.argv, "--no-runtime")
        os.execv(cmd[0], cmd)

    source_dir = os.getcwd()
    for package in args.packages:
        if os.path.isfile(package):
            spec = spk.api.read_spec_file(package)
            _LOGGER.info("saving spec file", pkg=spec.pkg)
            spk.save_spec(spec)
        else:
            spec = spk.load_spec(package)
            if not args.here:
                # FIXME: build binary package from source package
                raise NotImplementedError(
                    "No implementation yet to build from source package"
                )

        _LOGGER.info("building binary package", pkg=spec.pkg)
        out = spk.make_binary_package(spec, os.getcwd(), spk.api.OptionMap())
        _LOGGER.info("created", pkg=out)
