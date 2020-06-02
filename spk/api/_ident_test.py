from ._version import parse_version
from ._build import Build
from ._ident import Ident, parse_ident

import pytest
from ruamel import yaml


def test_ident_to_yaml() -> None:

    ident = Ident(name="package")
    out = yaml.safe_dump(  # type: ignore
        ident, default_flow_style=False, default_style='"'
    ).strip()
    assert out == '"package"'


@pytest.mark.parametrize(
    "input,expected",
    [
        ("hello/1.0.0/src", Ident("hello", parse_version("1.0.0"), Build("src"))),
        ("python/2.7", Ident("python", parse_version("2.7"))),
    ],
)
def test_parse_ident(input: str, expected: Ident) -> None:

    actual = parse_ident(input)
    assert actual == expected
