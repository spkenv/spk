import pytest

from ._version import parse_version
from ._compat import parse_compat


@pytest.mark.parametrize(
    "compat,a,b,expected",
    [
        ("x.x.x", "1.0.0", "1.0.0", True),
        ("x.x.a", "1.0.0", "1.0.2", True),
        ("x.x.x", "1.0.0", "1.0.2", False),
        # all prior numbers must be equal
        ("x.b.a", "1.0.0", "1.1.0", False),
        # compatible regardless of abi specification
        ("x.a", "1.0.0", "1.1.0", True),
        # not compatible if api compat is missing
        ("x.b", "1.0.0", "1.1.0", False),
        # compatible if both are provided
        ("x.ba", "1.0.0", "1.1.0", True),
    ],
)
def test_compat_api(compat: str, a: str, b: str, expected: bool) -> None:

    actual = parse_compat(compat).is_api_compatible(parse_version(a), parse_version(b))
    assert bool(actual) == expected


@pytest.mark.parametrize(
    "compat,a,b,expected",
    [
        ("x.x.x", "1.0.0", "1.0.0", True),
        ("x.x.b", "1.0.0", "1.0.2", True),
        ("x.x.x", "1.0.0", "1.0.2", False),
        ("x.b", "1.0.0", "1.1.0", True),
        ("x.a", "1.0.0", "1.1.0", False),
        ("x.a.b", "3.6.5", "3.7.1", False),
        ("x.a.b", "3.7.1", "3.7.5", True),
    ],
)
def test_compat_abi(compat: str, a: str, b: str, expected: bool) -> None:

    actual = parse_compat(compat).is_binary_compatible(
        parse_version(a), parse_version(b)
    )
    print(actual)
    assert bool(actual) == expected
