import pytest

from ._tag import TagSpec, Tag, decode_tag, split_tag_spec


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
        ("vfx2019", ("", "vfx2019", 0)),
        ("spi/base", ("spi", "base", 0)),
        ("spi/base~4", ("spi", "base", 4)),
        (
            "gitlab.spimageworks.com/spenv/spi/base",
            ("gitlab.spimageworks.com/spenv/spi", "base", 0),
        ),
    ],
)
def test_tag_spec_split(raw: str, expected: tuple) -> None:

    actual = split_tag_spec(raw)
    assert actual == expected


def test_tag_spec_class() -> None:

    src = "org/name~1"
    spec = TagSpec(src)
    assert isinstance(spec, str)
    assert f"{spec}" == src
    assert spec.org == "org"
    assert spec.name == "name"
    assert spec.version == 1


def test_tag_spec_path() -> None:

    spec = TagSpec("one_part")
    assert spec.path == "one_part"

    spec = TagSpec("two/parts")
    assert spec.path == "two/parts"


def test_tag_spec_validation() -> None:

    with pytest.raises(ValueError):
        TagSpec("")

    with pytest.raises(ValueError):
        TagSpec("name~-1")

    with pytest.raises(ValueError):
        TagSpec("name~1.23")
