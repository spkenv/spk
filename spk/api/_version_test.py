import pytest

from ._version import parse_version, Version, TagSet


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


@pytest.mark.parametrize(
    "string,expected",
    [
        ("1.0.0", Version(1, 0, 0)),
        ("0.0.0", Version(0, 0, 0)),
        ("1.2.3.4.5.6", Version(1, 2, 3, (4, 5, 6))),
        ("1.0+post.1", Version(1, 0, 0, post=TagSet({"post": 1}))),
        (
            "1.2.5.7-alpha.4+rev.6",
            Version(1, 2, 5, (7,), pre=TagSet({"alpha": 4}), post=TagSet({"post": 1})),
        ),
    ],
)
def test_parse_version(string: str, expected: Version) -> None:

    actual = parse_version(string)
    assert actual == expected


@pytest.mark.parametrize(
    "string", ["1.a.0", "my-version", "1.0+post.1-pre.2", "1.2.5-alpha.a"],
)
def test_parse_version_invalid(string: str) -> None:

    with pytest.raises(ValueError):
        parse_version(string)
