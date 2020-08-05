import os
from os.path import isdir

import pytest

from .. import api


from ._pip import _to_spk_version


@pytest.mark.parametrize(
    "version,expected",
    [
        ("1.0.0", "1.0.0"),
        ("1.0.dev456", "1.0.0-dev.456"),
        ("1.0a1", "1.0.0-a.1"),
        ("1.0a2.dev456", "1.0.0-a.2,dev.456"),
        ("1.0a12.dev456", "1.0.0-a.12,dev.456"),
        ("1.0a12", "1.0.0-a.12"),
        ("1.0b1.dev456", "1.0.0-b.1,dev.456"),
        ("1.0b2", "1.0.0-b.2"),
        ("1.0b2.post345.dev456", "1.0.0-b.2,dev.456+post.345"),
        ("1.0b2.post345", "1.0.0-b.2+post.345"),
        ("1.0rc1.dev456", "1.0.0-rc.1,dev.456"),
        ("1.0rc1", "1.0.0-rc.1"),
        ("1.0", "1.0.0"),
        ("1.0+abc.5", "1.0.0"),
        ("1.0+abc.7", "1.0.0"),
        ("1.0+5", "1.0.0"),
        ("1.0.post456.dev34", "1.0.0-dev.34+post.456"),
        ("1.0.post456", "1.0.0+post.456"),
        ("1.1.dev1", "1.1.0-dev.1"),
    ],
)
def test_version_conversion(version: str, expected: str) -> None:

    actual = _to_spk_version(version)
    assert actual == api.parse_version(expected)
