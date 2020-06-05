import pytest

from ._version import parse_version
from ._compat import parse_compat


@pytest.mark.parametrize(
    "compat,a,b,expected",
    [
        ("x.x.x", "1.0.0", "1.0.0", True),
        ("x.x.a", "1.0.0", "1.0.2", True),
        ("x.x.x", "1.0.0", "1.0.2", False),
    ],
)
def test_compat_compare(compat: str, a: str, b: str, expected: bool) -> None:

    actual = parse_compat(compat).check(parse_version(a), parse_version(b))
    assert actual == expected
