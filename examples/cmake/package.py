# Copyright (c) 2021 Sony Pictures Imageworks, et al.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/imageworks/spk

name = "SSPKCmakeExample"

# the package version number is updated automatically in this file
# during the spdev build process
version = "0.1.0"
description = ""
authors = []

# command line scripts and programs that this package exposes
tools = []

# package requirements are dependencies that needs to
# be included in any environment where this package
# is also included. These specifiers should be as
# loose as possible to avoid needing this package to
# be re-released for every minor update to its
# dependencies: eg use 'VnP3-2.*' rather than 'VnP3-2.1.6'
requires = []

# packages required when building this package or
# when this package is used in a downstream build.
# These might include header-only dependencies etc
build_requires = []

# required ONLY when this package is built directly, and
# not when it's included in another package's build. Build
# systems like cmake belong here, because downstream packages
# are not required to use it or have access to it when
# building against this package (even though they usually will)
private_build_requires = [
    "cmake-2.13+",
]

variants = [
    ["gcc-4.8"],
    ["gcc-6.3"],
]

build_system = "cmake"


def commands():
    # the root variable assumes that the cmake project name
    # is the same as the name of this package
    env.SKCmakeExample_ROOT = "{root}"
    env.LD_LIBRARY_PATH.append("{root}/lib")
    if building:
        env.CMAKE_MODULE_PATH.append("{root}")


uuid = "f0c2cd28-ab4f-4e4b-907b-4d442b789741"
