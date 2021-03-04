from typing import Any
import argparse
import os

import structlog

from . import _flags
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
        "packages",
        metavar="PKG|SPEC_FILE",
        nargs="+",
        help="The packages or yaml specification files to build",
    )
    _flags.add_no_runtime_flag(make_source_cmd)
    make_source_cmd.set_defaults(func=_make_source)
    return make_source_cmd


def _make_source(args: argparse.Namespace) -> None:
    """Build a source package from a spec file."""

    _flags.ensure_active_runtime(args)

    for package in args.packages:
        if os.path.isfile(package):
            spec = spk.api.read_spec_file(package)
            _LOGGER.info("saving spec file", pkg=spec.pkg)
            spk.save_spec(spec)
        else:
            spec = spk.load_spec(package)

        _LOGGER.info("collecting sources", pkg=spec.pkg)
        out = spk.SourcePackageBuilder.from_spec(spec).build()
        _LOGGER.info("created", pkg=out)
