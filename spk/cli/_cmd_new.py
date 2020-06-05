from typing import Any
import argparse
import os
import textwrap

import structlog

import spfs
import spk

from spk.io import format_decision

_LOGGER = structlog.get_logger("cli")


def register(
    sub_parsers: argparse._SubParsersAction, **parser_args: Any
) -> argparse.ArgumentParser:

    new_cmd = sub_parsers.add_parser("new", help=_new.__doc__, **parser_args)
    new_cmd.add_argument(
        "name", metavar="NAME", nargs=1, help="The name of the new package"
    )
    new_cmd.set_defaults(func=_new)
    return new_cmd


def _new(args: argparse.Namespace) -> None:
    """Generate a new package spec file."""

    name = args.name[0]

    spec = f"""\
        pkg: {name}/0.1.0

        # opts defines the set of build options
        opts:
          # var options define environment/string values that affect the
          # package build process. The value is defined in the build environment
          # as SPK_OPT_{{name}}
          - var: arch    # rebuild if the arch changes
          - var: os      # rebuild if the os changes
          - var: centos  # rebuild if centos version changes
          # declaring options prefixed by this pacakges name signals
          # to others that they are not global build settings for any package
          # - var: {name}_debug # toggle a debug build of this package

          # pkg options request packages that need to be present
          # in the build environment
          - pkg: build_requirement/1.0

        # depends:
          # pkg dependencies request packages that need to be present at runtime
          # - pkg: dependency/1.0
        """
    # TODO: talk about pinning build env packages once supported
    spec = textwrap.dedent(spec)

    spec_file = f"{name}.yaml"
    with open(spec_file, "x") as writer:
        writer.write(spec)
    print("created:", spec_file)
