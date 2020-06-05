import pytest

from ._version import parse_version, Version


def test_version_nonzero() -> None:

    assert bool(Version()) == False
    assert bool(Version(1, 0, 0)) == True


@pytest.mark.parametrize(
    "base,test,expected",
    [
        ("1.0.0", "1.0.0", False),
        ("1", "1.0.0", False),
        ("1.0.0", "1", False),
        ("6.3", "4.8.5", True),
    ],
)
def test_is_gt(base: str, test: str, expected: bool) -> None:

    assert (parse_version(base) > parse_version(test)) == expected
