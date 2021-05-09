# Copyright (c) 2021 Sony Pictures Imageworks, et al.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/imageworks/spk

from typing import Dict
import pytest

from ._version import parse_version
from ._version_range import parse_version_range
from ._spec import Spec


def test_parse_version_range_carat() -> None:

    vr = parse_version_range("^1.0.1")
    assert vr.greater_or_equal_to() == "1.0.1"
    assert vr.less_than() == "2.0.0"


def test_parse_version_range_tilde() -> None:

    vr = parse_version_range("~1.0.1")
    assert vr.greater_or_equal_to() == "1.0.1"
    assert vr.less_than() == "1.1.0"

    with pytest.raises(ValueError):
        parse_version_range("~2")


@pytest.mark.parametrize(
    "range,version,expected",
    [
        ("~1.0.0", "1.0.0", True),
        ("~1.0.0", "1.0.1", True),
        ("~1.0.0", "1.2.1", False),
        ("^1.0.0", "1.0.0", True),
        ("^1.0.0", "1.1.0", True),
        ("^1.0.0", "1.0.1", True),
        ("^1.0.0", "2.0.0", False),
        ("^0.1.0", "2.0.0", False),
        ("^0.1.0", "0.2.0", False),
        ("^0.1.0", "0.1.4", True),
        ("1.0.*", "1.0.0", True),
        ("1.*", "1.0.0", True),
        ("1.*", "1.4.6", True),
        ("1.*.0", "1.4.6", False),
        ("1.*.0", "1.4.0", True),
        ("*", "100.0.0", True),
        (">1.0.0", "1.0.0", False),
        ("<1.0.0", "1.0.0", False),
        ("<=1.0.0", "1.0.0", True),
        ("<=1.0.0", "1.0.1", False),
        (">=1.0.0", "1.0.1", True),
        ("1.0.0", "1.0.0", True),
        ("1.0.0", "1.0.0", True),
        ("!=1.0", "1.0.1", False),
        ("!=1.0", "1.1.0", True),
        ("=1.0.0", "1.0.0", True),
        ("=1.0.0", "1.0.0+r.1", True),
        ("=1.0.0+r.2", "1.0.0+r.1", False),
    ],
)
def test_version_range_is_applicable(range: str, version: str, expected: bool) -> None:

    vr = parse_version_range(range)
    v = parse_version(version)
    actual = vr.is_applicable(v)

    assert bool(actual) == expected, actual


@pytest.mark.parametrize(
    "range,spec_dict,expected",
    [
        ("=1.0.0", {"pkg": "test/1.0.0"}, True),
        ("=1.0.0", {"pkg": "test/1.0.0+r.1"}, True),
        ("=1.0.0+r.2", {"pkg": "test/1.0.0+r.1"}, False),
    ],
)
def test_version_range_is_satisfied(
    range: str, spec_dict: Dict, expected: bool
) -> None:

    vr = parse_version_range(range)
    spec = Spec.from_dict(spec_dict)
    actual = vr.is_satisfied_by(spec)

    assert bool(actual) == expected, actual
