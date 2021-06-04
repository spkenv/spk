# Copyright (c) 2021 Sony Pictures Imageworks, et al.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/imageworks/spk

import os
from typing import Dict

import pytest

from .. import api


from ._spcomp2 import _build_to_options, SPCOMP2_EXCLUDED_BUILDS, SPCOMP2_ROOT


@pytest.mark.parametrize(
    "build,expected",
    [
        (
            "rhel7-gcc63-boost170",
            {
                "centos": "7",
                "distro": "centos",
                "os": "linux",
                "gcc": "6.3",
                "boost": "1.70",
                "arch": "x86_64",
            },
        ),
        (
            "rhel40-gcc402m64",
            {
                "rhel": "40",
                "distro": "rhel",
                "os": "linux",
                "gcc": "4.02",
                "arch": "x86_64",
            },
        ),
        (
            "rhel7-gcc63-ice36",
            {
                "centos": "7",
                "distro": "centos",
                "os": "linux",
                "gcc": "6.3",
                "arch": "x86_64",
                "ice": "3.6",
            },
        ),
        (
            "spinux1-gcc44m64",
            {
                "spinux": "1",
                "distro": "spinux",
                "os": "linux",
                "gcc": "4.4",
                "arch": "x86_64",
            },
        ),
        (
            "rhel40-gcc34",
            {
                "rhel": "40",
                "distro": "rhel",
                "os": "linux",
                "gcc": "3.4",
                "arch": "x86_64",
            },
        ),
        (
            "rhel40-gcc42m64",
            {
                "rhel": "40",
                "distro": "rhel",
                "os": "linux",
                "gcc": "4.2",
                "arch": "x86_64",
            },
        ),
        (
            "spinux1-gcc412m64",
            {
                "spinux": "1",
                "distro": "spinux",
                "os": "linux",
                "gcc": "4.12",
                "arch": "x86_64",
            },
        ),
        (
            "rhel40-gcc34m64",
            {
                "rhel": "40",
                "distro": "rhel",
                "os": "linux",
                "gcc": "3.4",
                "arch": "x86_64",
            },
        ),
        (
            "rhel7-gcc48m64-ice36",
            {
                "centos": "7",
                "distro": "centos",
                "os": "linux",
                "gcc": "4.8",
                "arch": "x86_64",
                "ice": "3.6",
            },
        ),
        (
            "spinux1-gcc41m64",
            {
                "spinux": "1",
                "distro": "spinux",
                "os": "linux",
                "gcc": "4.1",
                "arch": "x86_64",
            },
        ),
    ],
)
def test_build_to_options(build: str, expected: Dict[str, str]) -> None:

    expected = api.OptionMap(expected)
    actual = _build_to_options(build)
    spec = api.BuildSpec(options=actual)
    compat = spec.validate_options("", expected)
    assert compat, compat


try:
    names = os.listdir(SPCOMP2_ROOT)
except FileNotFoundError:
    names = []

all_builds = set()
for name in names:
    build_dir = os.path.join(SPCOMP2_ROOT, name)
    if not os.path.isdir(build_dir):
        continue
    for build in os.listdir(build_dir):
        if not os.path.isdir(os.path.join(build_dir, build)):
            continue
        all_builds.add(build)


@pytest.mark.parametrize("build", list(all_builds))
def test_build_to_options_all_cases(build: str) -> None:

    if "-" not in build:
        pytest.skip("must include a dash to be considered")
    for excl in SPCOMP2_EXCLUDED_BUILDS:
        if excl in build:
            pytest.skip("build is excluded")

    # should not raise - but we won't check each one definitively
    _build_to_options(build)
