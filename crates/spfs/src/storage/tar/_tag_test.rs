import pytest
import py.path
import tarfile

from ... import tracking, encoding
from ._tag import TagStorage


@pytest.mark.timeout(1)
def test_tag_stream(tmpdir: py.path.local) -> None:

    storage = TagStorage(tarfile.open(tmpdir.join("db.tar").strpath, "a"))

    h = encoding.Hasher()
    digest1 = h.digest()
    h.update(b"hello")
    digest2 = h.digest()

    tag1 = storage.push_tag("hello/world", digest1)
    assert storage.resolve_tag("hello/world") == tag1
    assert storage.resolve_tag("hello/world~0") == tag1


def test_ls_tags(tmpdir: py.path.local) -> None:

    storage = TagStorage(tarfile.open(tmpdir.join("tags").strpath, "a"))
    for tag in (
        "spi/stable/my_tag",
        "spi/stable/other_tag",
        "spi/latest/my_tag",
    ):
        storage.push_tag(tag, encoding.EMPTY_DIGEST)

    assert sorted(storage.ls_tags("/")) == ["spi"]
    assert sorted(storage.ls_tags("/spi")) == ["latest", "stable"]
    assert sorted(storage.ls_tags("spi/stable")) == ["my_tag", "other_tag"]
