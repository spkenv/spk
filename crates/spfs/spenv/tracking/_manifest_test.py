from typing import OrderedDict
import os
import random

import pytest

from ._manifest import Entry, EntryKind, compute_tree, compute_manifest


def test_entry_blobs_compare_name():

    a = Entry(name="a", kind=EntryKind.BLOB, mode=0, digest="")
    b = Entry(name="b", kind=EntryKind.BLOB, mode=0, digest="")
    assert a < b and b > a


def test_entry_trees_compare_name():

    a = Entry(name="a", kind=EntryKind.TREE, mode=0, digest="")
    b = Entry(name="b", kind=EntryKind.TREE, mode=0, digest="")
    assert a < b and b > a


def test_entry_compare_kind():

    blob = Entry(name="a", kind=EntryKind.BLOB, mode=0, digest="")
    tree = Entry(name="b", kind=EntryKind.TREE, mode=0, digest="")
    assert tree > blob and blob < tree


def test_compute_tree_determinism():

    first = compute_tree("./spenv")
    second = compute_tree("./spenv")
    assert first == second


def test_compute_manifest():

    manifest = compute_manifest(os.path.abspath("./spenv"))
    assert manifest.get_path(__file__)


def test_manifest_relative_paths(tmpdir) -> None:

    tmpdir.join("dir1.0/dir2.0/file.txt").write("somedata", ensure=True)
    tmpdir.join("dir1.0/dir2.1/file.txt").write("someotherdata", ensure=True)
    tmpdir.join("dir2.0/file.txt").write("evenmoredata", ensure=True)
    tmpdir.join("a_file.txt").write("rootdata", ensure=True)

    manifest = compute_manifest(tmpdir.strpath)
    assert manifest.get_path(".") is not None
    assert manifest.get_path("dir1.0/dir2.0/file.txt") is not None
    assert manifest.get_path("./dir1.0/dir2.1/file.txt") is not None
    assert manifest.get_path(tmpdir.join("dir1.0/dir2.0/file.txt").strpath) is not None


def test_manifest_sorting(tmpdir):

    tmpdir.join("dir1.0/dir2.0/file.txt").write("somedata", ensure=True)
    tmpdir.join("dir1.0/dir2.1/file.txt").write("someotherdata", ensure=True)
    tmpdir.join("dir1.0/file.txt").write("thebestdata", ensure=True)
    tmpdir.join("dir2.0/file.txt").write("evenmoredata", ensure=True)
    tmpdir.join("a_file.txt").write("rootdata", ensure=True)
    tmpdir.join("z_file.txt").write("rootdata", ensure=True)

    manifest = compute_manifest(tmpdir.strpath)

    items = list(manifest._paths.items())
    random.shuffle(items)
    manifest._paths = OrderedDict(items)

    manifest.sort()
    actual = [p for p, _ in manifest.walk()]
    expected = [
        ".",
        "./a_file.txt",
        "./z_file.txt",
        "./dir1.0",
        "./dir1.0/file.txt",
        "./dir1.0/dir2.0",
        "./dir1.0/dir2.0/file.txt",
        "./dir1.0/dir2.1",
        "./dir1.0/dir2.1/file.txt",
        "./dir2.0",
        "./dir2.0/file.txt",
    ]
    assert actual == expected
