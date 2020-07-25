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


TEMPLATE = """\
pkg: {name}/0.1.0

build:

  # options are all the inputs to the package build process, including
  # build-time dependencies
  options:
    # var options define environment/string values that affect the build.
    # The value is defined in the build environment as SPK_OPT_{{name}}
    - var: arch    # rebuild if the arch changes
    - var: os      # rebuild if the os changes
    - var: centos  # rebuild if centos version changes

    # pkg options request packages that need to be present
    # in the build environment. You can specify a version number
    # here as the default for when the option is not otherise specified
    - pkg: python
      default: 3

  # variants declares the default set of variants to build and publish
  # using the spk build and make-* commands
  variants:
    - {{python: 2.7}}
    - {{python: 3.7}}
    # you can also force option values for specific dependencies with a prefix
    # - {{python: 2.7, vnp3.debug: on}}

  # the build script is arbitrary bash script to be executed for the
  # build. It should be and install artifacts into /spfs
  script:
    # if you remove this it will try to run a build.sh script instead
    - echo "don't forget to add build logic!"
    - exit 1

install:
  requirements:
    - pkg: python
      # we can use the version of python from the build environment to dynamically
      # define the install requirement
      fromBuildEnv: x.x
"""


def _new(args: argparse.Namespace) -> None:
    """Generate a new package spec file."""

    name = spk.api.validate_name(args.name[0])
    spec = TEMPLATE.format(name=name)

    spec_file = f"{name}.spk.yaml"
    with open(spec_file, "x") as writer:
        writer.write(spec)
    print("created:", spec_file)
