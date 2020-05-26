from .. import compat
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
        ("hello/1.0.0/src", Ident("hello", compat.Version("1.0.0"), Build("src"))),
        ("python/2.7", Ident("python", compat.Version("2.7"))),
    ],
)
def test_parse_ident(input: str, expected: Ident) -> None:

    actual = parse_ident(input)
    assert actual == expected


# def test_ident_from_yaml() -> None:

#     spec := Ident{}
#     err := yaml.Unmarshal([]byte("package/1.0.2/r2"), &spec)
#     if err != nil {
#         t.Fatal(err)
#     }
#     if spec.Name != "package" {
#         t.Errorf("expected package name to be separated out: (%s != %s)", spec.Name, "package")
#     }
#     if spec.Version.String() != "1.0.2" {
#         t.Errorf("expected package version to be separated out: (%s != %s)", spec.Version, "1.0.2")
#     }
#     if spec.Build.String() != "r2" {
#         t.Errorf("expected package release to be separated out: (%s != %s)", spec.Build, "r2")
#     }
