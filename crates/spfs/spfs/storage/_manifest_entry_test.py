from .. import encoding, tracking
from ._manifest import Entry


def test_entry_blobs_compare_name() -> None:

    a = Entry(
        name="a",
        entry=tracking.Entry(
            kind=tracking.EntryKind.BLOB,
            mode=0,
            object=encoding.EMPTY_DIGEST,
            size=0,
        ),
    )
    b = Entry(
        name="b",
        entry=tracking.Entry(
            kind=tracking.EntryKind.BLOB,
            mode=0,
            object=encoding.EMPTY_DIGEST,
            size=0,
        ),
    )
    assert a < b and b > a


def test_entry_trees_compare_name() -> None:

    a = Entry(
        name="a",
        entry=tracking.Entry(
            kind=tracking.EntryKind.TREE,
            mode=0,
            object=encoding.EMPTY_DIGEST,
            size=0,
        ),
    )
    b = Entry(
        name="b",
        entry=tracking.Entry(
            kind=tracking.EntryKind.TREE,
            mode=0,
            object=encoding.EMPTY_DIGEST,
            size=0,
        ),
    )
    assert a < b and b > a


def test_entry_compare_kind() -> None:

    blob = Entry(
        name="a",
        entry=tracking.Entry(
            kind=tracking.EntryKind.BLOB,
            mode=0,
            object=encoding.EMPTY_DIGEST,
            size=0,
        ),
    )
    tree = Entry(
        name="b",
        entry=tracking.Entry(
            kind=tracking.EntryKind.TREE,
            mode=0,
            object=encoding.EMPTY_DIGEST,
            size=0,
        ),
    )
    assert tree > blob and blob < tree


def test_entry_compare() -> None:

    defaults = {"mode": 0, "object": "", "size": 0}
    root_file = Entry(
        name="file", entry=tracking.Entry(kind=tracking.EntryKind.BLOB, **defaults)  # type: ignore
    )
    root_dir = Entry(
        name="xdir", entry=tracking.Entry(kind=tracking.EntryKind.TREE, **defaults)  # type: ignore
    )
    assert root_dir > root_file
