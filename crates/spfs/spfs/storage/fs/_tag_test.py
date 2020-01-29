import py.path

from ... import tracking
from ._tag import TagStorage


def test_tag_stream(tmpdir: py.path.local) -> None:

    storage = TagStorage(tmpdir.strpath)

    tag1 = storage.push_tag("hello/world", "----digest1----")
    assert storage.resolve_tag("hello/world") == tag1
    assert storage.resolve_tag("hello/world~0") == tag1

    tag2 = storage.push_tag("hello/world", "----digest2----")
    tag2 = storage.push_tag("hello/world", "----digest2----")
    assert storage.resolve_tag("hello/world") == tag2
    assert storage.resolve_tag("hello/world~0") == tag2
    assert storage.resolve_tag("hello/world~1") == tag1
    assert tuple(storage.find_tags("----digest2----")) == ("hello/world",)
    assert tuple(storage.find_tags("----digest1----")) == ("hello/world~1",)


def test_tag_no_duplication(tmpdir: py.path.local) -> None:

    storage = TagStorage(tmpdir.join("tags").strpath)
    tag1 = storage.push_tag("hello", "my_target")
    tag2 = storage.push_tag("hello", "my_target")

    assert tag1 == tag2
    assert len(list(storage.read_tag("hello"))) == 1
