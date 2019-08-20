import pytest

from ._tag import Tag


@pytest.mark.parametrize(
    "raw,expected",
    [
        ("vfxreference:2019", Tag(name="vfxreference", version="2019")),
        ("vfxreference", Tag(name="vfxreference", version="latest")),
        ("spi/base", Tag(org="spi", name="base", version="latest")),
        ("spi/base:25.6", Tag(org="spi", name="base", version="25.6")),
        (
            "gitlab.spimageworks.com/spenv/spi/base",
            Tag(org="gitlab.spimageworks.com/spenv/spi", name="base"),
        ),
    ],
)
def test_Tag_parse(raw: str, expected: Tag):

    actual = Tag.parse(raw)
    assert actual == expected
