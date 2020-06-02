import pytest

from ._version import parse_version


@pytest.mark.parametrize(
    "base,test,expected",
    [
        ("1.0.0", "1.0.0", True),
        ("1", "1.0.0", True),
        ("1.0.0", "1", False),
        ("6.3", "4.8.5", False),
    ],
)
def test_is_satisfied_by(base: str, test: str, expected: bool) -> None:

    assert parse_version(base).is_satisfied_by(test) == expected
