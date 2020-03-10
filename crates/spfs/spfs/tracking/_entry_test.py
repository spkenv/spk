from .. import encoding
from ._entry import Entry, EntryKind


def test_entry_blobs_compare_name() -> None:

    a = Entry(name="a", kind=EntryKind.BLOB, mode=0, object=encoding.EMPTY_DIGEST)
    b = Entry(name="b", kind=EntryKind.BLOB, mode=0, object=encoding.EMPTY_DIGEST)
    assert a < b and b > a


def test_entry_trees_compare_name() -> None:

    a = Entry(name="a", kind=EntryKind.TREE, mode=0, object=encoding.EMPTY_DIGEST)
    b = Entry(name="b", kind=EntryKind.TREE, mode=0, object=encoding.EMPTY_DIGEST)
    assert a < b and b > a


def test_entry_compare_kind() -> None:

    blob = Entry(name="a", kind=EntryKind.BLOB, mode=0, object=encoding.EMPTY_DIGEST)
    tree = Entry(name="b", kind=EntryKind.TREE, mode=0, object=encoding.EMPTY_DIGEST)
    assert tree > blob and blob < tree


def test_entry_compare() -> None:

    defaults = {"mode": 0, "object": ""}
    root_file = Entry(name="file", kind=EntryKind.BLOB, **defaults)  # type: ignore
    root_dir = Entry(name="xdir", kind=EntryKind.TREE, **defaults)  # type: ignore
    assert root_dir > root_file
