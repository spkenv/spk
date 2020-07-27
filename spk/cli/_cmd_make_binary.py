from typing import Callable, Any
import argparse
import os
import sys

import spfs
import structlog
from colorama import Fore

import spk
from . import _flags

_LOGGER = structlog.get_logger("spk.cli")


def register(
    sub_parsers: argparse._SubParsersAction, **parser_args: Any
) -> argparse.ArgumentParser:

    mkb_cmd = sub_parsers.add_parser(
        "make-binary",
        aliases=["mkbinary", "mkbin", "mkb"],
        help=_make_binary.__doc__,
        **parser_args
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
    _flags.add_repo_flags(mkb_cmd)
    _flags.add_option_flags(mkb_cmd)
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

        base_options = spk.api.host_options()
        options = _flags.get_options_from_flags(args)
        variants = spec.build.variants
        if options:
            variants = [options]
        repos = _flags.get_repos_from_repo_flags(args).values()
        _LOGGER.info("building binary package", pkg=spec.pkg)
        for variant in variants:

            _LOGGER.info("building variant", variant=variant)
            builder = (
                spk.BinaryPackageBuilder.from_spec(spec)
                .with_options(base_options)
                .with_options(variant)
                .with_repositories(repos)
            )
            if args.here:
                builder = builder.with_source(os.getcwd())
            try:
                out = builder.build()
            except spk.SolverError:
                if args.verbose:
                    tree = builder.get_build_env_decision_tree()
                    print(spk.io.format_decision_tree(tree, verbosity=args.verbose))
                raise
            else:
                _LOGGER.info("created", pkg=out)
