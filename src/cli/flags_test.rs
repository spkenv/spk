# Copyright (c) 2021 Sony Pictures Imageworks, et al.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/imageworks/spk
from typing import Dict, List, Type
import pytest
import argparse

import spk
from . import _flags


@pytest.mark.parametrize(
    "args,expected",
    [
        (["-o", "hello:world"], {"hello": "world"}),
        (["-o", "hello=world"], {"hello": "world"}),
        (["-o", "{hello: world}"], {"hello": "world"}),
        (["-o", "{python: 2.7}"], {"python": "2.7"}),
        (
            ["-o", "{python: 2.7, python.abi: py37m}"],
            {"python": "2.7", "python.abi": "py37m"},
        ),
    ],
)
def test_option_flags_parsing(args: List[str], expected: Dict[str, str]) -> None:

    parser = argparse.ArgumentParser("tester")
    _flags.add_option_flags(parser)
    parsed = parser.parse_args(["--no-host"] + args)
    opts = _flags.get_options_from_flags(parsed)
    assert opts == spk.api.OptionMap(expected)


@pytest.mark.parametrize(
    "args,expected",
    [
        (["-o", "{hello: [world]}"], AssertionError),
        (["-o", "{python: {v: 2.7}}"], AssertionError),
        (["-o", "value"], ValueError),
    ],
)
def test_option_flags_parsing_err(args: List[str], expected: Type[Exception]) -> None:
    with pytest.raises(expected):
        parser = argparse.ArgumentParser("tester")
        _flags.add_option_flags(parser)
        parsed = parser.parse_args(args)
        opts = _flags.get_options_from_flags(parsed)
