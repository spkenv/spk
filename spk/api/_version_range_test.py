import pytest

from ._version import parse_version
from ._version_range import VersionRange, parse_version_range


def test_parse_version_range_carat() -> None:

    vr = parse_version_range("^1.0.1")
    assert vr.greater_or_equal_to() == "1.0.1"
    assert vr.less_than() == "2.0.0"


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
        ("1.0.*", "1.0.0", True),
        ("1.*.*", "1.0.0", True),
        ("1.*.*", "1.4.6", True),
        ("1.*.0", "1.4.6", False),
        ("1.*.0", "1.4.0", True),
        (">1.0.0", "1.0.0", False),
        ("<1.0.0", "1.0.0", False),
        ("<=1.0.0", "1.0.0", True),
        ("<=1.0.0", "1.0.1", False),
        (">=1.0.0", "1.0.1", True),
        ("1.0.0", "1.0.0", True),
        ("1.0.0", "1.0.0", True),
        ("!=1.0", "1.0.1", False),
        ("!=1.0", "1.1.0", True),
    ],
)
def test_version_range_is_applicable(range: str, version: str, expected: bool) -> None:

    vr = parse_version_range(range)
    v = parse_version(version)
    actual = vr.is_applicable(v)

    assert bool(actual) == expected
