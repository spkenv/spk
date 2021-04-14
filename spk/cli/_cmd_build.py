from spk import build
from typing import Any
import argparse
import subprocess

import structlog

from . import _flags

_LOGGER = structlog.get_logger("cli")


def register(
    sub_parsers: argparse._SubParsersAction, **parser_args: Any
) -> argparse.ArgumentParser:

    build_cmd = sub_parsers.add_parser("build", help=_build.__doc__, **parser_args)
    build_cmd.add_argument(
        "--interactive",
        "-i",
        action="store_true",
        help="Setup the build, but instead of running the build script start an interactive shell",
    )
    build_cmd.add_argument(
        "--env",
        "-e",
        action="store_true",
        help="Build the first variant of this package, and then immediately enter a shell environment with it",
    )
    build_cmd.add_argument(
        "files",
        metavar="SPEC_FILE",
        nargs="*",
        default=[""],
        help="The package(s) to build",
    )
    _flags.add_repo_flags(build_cmd)
    _flags.add_option_flags(build_cmd)
    build_cmd.set_defaults(func=_build)
    return build_cmd


def _build(args: argparse.Namespace) -> None:
    """Runs make-source and then make-binary."""

    common_args = []
    if args.verbose:
        common_args += ["-" + "v" * args.verbose]

    for filename in args.files:
        _spec, filename = _flags.find_package_spec(filename)

        cmd = ["spk", "make-source", filename, *common_args]
        _LOGGER.info(" ".join(cmd))
        with build.deferred_signals():
            proc = subprocess.Popen(cmd)
            proc.wait()
        if proc.returncode != 0:
            raise SystemExit(proc.returncode)
        binary_flags = []
        for option in args.opt:
            binary_flags.extend(["-o", option])
        if args.no_host:
            binary_flags.append("--no_host")
        if args.env:
            binary_flags.append("-e")
        if args.local_repo:
            binary_flags.append("-l")
        for r in args.enable_repo:
            binary_flags.extend(["-r", r])
        for r in args.disable_repo:
            binary_flags.extend(["-dr", r])
        if args.interactive:
            binary_flags.append("-i")
        cmd = ["spk", "make-binary", filename, *common_args, *binary_flags]
        _LOGGER.info(" ".join(cmd))
        with build.deferred_signals():
            proc = subprocess.Popen(cmd)
            proc.wait()
        if proc.returncode != 0:
            raise SystemExit(proc.returncode)
