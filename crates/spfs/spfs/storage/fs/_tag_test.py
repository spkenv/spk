import pytest
import py.path

from ... import tracking, encoding
from ._tag import TagStorage


@pytest.mark.timeout(1)
def test_tag_stream(tmpdir: py.path.local) -> None:

    storage = TagStorage(tmpdir.strpath)

    h = encoding.Hasher()
    digest1 = h.digest()
    h.update(b"hello")
    digest2 = h.digest()

    tag1 = storage.push_tag("hello/world", digest1)
    assert storage.resolve_tag("hello/world") == tag1
    assert storage.resolve_tag("hello/world~0") == tag1

    tag2 = storage.push_tag("hello/world", digest2)
    tag2 = storage.push_tag("hello/world", digest2)
    assert storage.resolve_tag("hello/world") == tag2
    assert storage.resolve_tag("hello/world~0") == tag2
    assert storage.resolve_tag("hello/world~1") == tag1
    assert tuple(storage.find_tags(digest2)) == ("hello/world",)
    assert tuple(storage.find_tags(digest1)) == ("hello/world~1",)


def test_tag_no_duplication(tmpdir: py.path.local) -> None:

    storage = TagStorage(tmpdir.join("tags").strpath)
    tag1 = storage.push_tag("hello", encoding.EMPTY_DIGEST)
    tag2 = storage.push_tag("hello", encoding.EMPTY_DIGEST)

    assert tag1 == tag2
    assert len(list(storage.read_tag("hello"))) == 1


def test_ls_tags(tmpdir: py.path.local) -> None:

    storage = TagStorage(tmpdir.join("tags").strpath)
    for tag in (
        "spi/stable/my_tag",
        "spi/stable/other_tag",
        "spi/latest/my_tag",
    ):
        storage.push_tag(tag, encoding.EMPTY_DIGEST)

    assert storage.ls_tags("/") == ["spi"]
    assert storage.ls_tags("/spi") == ["stable", "latest"]
    assert storage.ls_tags("spi/stable") == ["my_tag", "other_tag"]
