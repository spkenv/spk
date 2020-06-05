from typing import Any
import argparse
import os
import subprocess

import structlog

import spfs
import spk

from spk.io import format_decision

_LOGGER = structlog.get_logger("cli")


def register(
    sub_parsers: argparse._SubParsersAction, **parser_args: Any
) -> argparse.ArgumentParser:

    build_cmd = sub_parsers.add_parser("build", help=_build.__doc__, **parser_args)
    build_cmd.add_argument(
        "name", metavar="SPEC_FILE", nargs=1, help="The package spec file to build"
    )
    build_cmd.set_defaults(func=_build)
    return build_cmd


def _build(args: argparse.Namespace) -> None:
    """Runs make-source and then make-binary."""

    spec = spk.read_spec_file(args.name[0])

    cmd = ["spk", "make-source", args.name[0]]
    _LOGGER.info(" ".join(cmd))
    proc = subprocess.Popen(cmd)
    proc.wait()
    if proc.returncode != 0:
        raise SystemExit(proc.returncode)
    # FIXME: do not use here argument in the future when src build are setup
    cmd = ["spk", "make-binary", "--here", str(spec.pkg.with_build(None))]
    _LOGGER.info(" ".join(cmd))
    proc = subprocess.Popen(cmd)
    proc.wait()
    if proc.returncode != 0:
        raise SystemExit(proc.returncode)
