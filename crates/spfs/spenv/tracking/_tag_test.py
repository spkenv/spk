import pytest

from ._tag import TagSpec, Tag, decode_tag, parse_tag_spec


@pytest.mark.parametrize(
    "tag", [Tag(org="vfx", name="2019", target="------digest------")]
)
def test_tag_encoding(tag: Tag) -> None:

    encoded = tag.encode()
    decoded = decode_tag(encoded)
    assert tag.digest == decoded.digest


@pytest.mark.parametrize(
    "raw,expected",
    [
        ("vfx2019", TagSpec(name="vfx2019")),
        ("spi/base", TagSpec(org="spi", name="base")),
        ("spi/base[-4]", TagSpec(org="spi", name="base", version=-4)),
        (
            "gitlab.spimageworks.com/spenv/spi/base",
            TagSpec(org="gitlab.spimageworks.com/spenv/spi", name="base"),
        ),
    ],
)
def test_tag_spec_parse(raw: str, expected: TagSpec) -> None:

    actual = parse_tag_spec(raw)
    assert actual == expected


def test_tag_spec_path() -> None:

    spec = parse_tag_spec("one_part")
    assert spec.path == "one_part"

    spec = parse_tag_spec("two/parts")
    assert spec.path == "two/parts"
