from typing import Callable, Any
import argparse
import os
import sys

import spfs
import structlog
from colorama import Fore

import spk

_LOGGER = structlog.get_logger("spk.cli")


def register(
    sub_parsers: argparse._SubParsersAction, **parser_args: Any
) -> argparse.ArgumentParser:

    make_source_cmd = sub_parsers.add_parser(
        "make-source",
        aliases=["mksource", "mksrc", "mks"],
        help=_make_source.__doc__,
        **parser_args,
    )
    make_source_cmd.add_argument(
        "--no-runtime",
        "-nr",
        action="store_true",
        help="Do not build in a new spfs runtime for debugging (will reset the current runtime)",
    )
    make_source_cmd.add_argument(
        "packages",
        metavar="PKG|SPEC_FILE",
        nargs="+",
        help="The packages or yaml specification files to build",
    )
    make_source_cmd.set_defaults(func=_make_source)
    return make_source_cmd


def _make_source(args: argparse.Namespace) -> None:
    """Build a source package from a spec file."""

    if not args.no_runtime:
        runtime = spfs.get_config().get_runtime_storage().create_runtime()
        runtime.set_editable(True)
        cmd = spfs.build_command_for_runtime(runtime, *sys.argv, "--no-runtime")
        os.execv(cmd[0], cmd)

    for package in args.packages:
        if os.path.isfile(package):
            spec = spk.api.read_spec_file(package)
            _LOGGER.info("saving spec file", pkg=spec.pkg)
            spk.save_spec(spec)
        else:
            spec = spk.load_spec(package)

        _LOGGER.info("collecting sources", pkg=spec.pkg)
        out = spk.make_source_package(spec)
        _LOGGER.info("created", pkg=out)
