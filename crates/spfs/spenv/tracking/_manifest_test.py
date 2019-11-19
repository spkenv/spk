from collections import OrderedDict
import os
import random

import py.path
import pytest

from ._manifest import (
    MutableManifest,
    Manifest,
    Entry,
    EntryKind,
    compute_tree,
    compute_entry,
    compute_manifest,
    layer_manifests,
)
from ._diff import compute_diff


def test_entry_blobs_compare_name() -> None:

    a = Entry(name="a", kind=EntryKind.BLOB, mode=0, object="")
    b = Entry(name="b", kind=EntryKind.BLOB, mode=0, object="")
    assert a < b and b > a


def test_entry_trees_compare_name() -> None:

    a = Entry(name="a", kind=EntryKind.TREE, mode=0, object="")
    b = Entry(name="b", kind=EntryKind.TREE, mode=0, object="")
    assert a < b and b > a


def test_entry_compare_kind() -> None:

    blob = Entry(name="a", kind=EntryKind.BLOB, mode=0, object="")
    tree = Entry(name="b", kind=EntryKind.TREE, mode=0, object="")
    assert tree > blob and blob < tree


def test_compute_tree_determinism() -> None:

    first = compute_tree("./spenv")
    second = compute_tree("./spenv")
    assert first == second


def test_compute_manifest() -> None:

    root = os.path.abspath("./spenv")
    this = os.path.relpath(__file__, root)
    manifest = compute_manifest(root)
    assert manifest.get_path(this) is not None


def test_manifest_relative_paths(tmpdir: py.path.local) -> None:

    tmpdir.join("dir1.0/dir2.0/file.txt").write("somedata", ensure=True)
    tmpdir.join("dir1.0/dir2.1/file.txt").write("someotherdata", ensure=True)
    tmpdir.join("dir2.0/file.txt").write("evenmoredata", ensure=True)
    tmpdir.join("a_file.txt").write("rootdata", ensure=True)

    manifest = compute_manifest(tmpdir.strpath)
    assert manifest.get_path("/") is not None
    assert manifest.get_path("/dir1.0/dir2.0/file.txt") is not None
    assert manifest.get_path("dir1.0/dir2.1/file.txt") is not None


def test_entry_compare() -> None:

    defaults = {"mode": 0, "object": ""}
    root_file = Entry(name="file", kind=EntryKind.BLOB, **defaults)  # type: ignore
    root_dir = Entry(name="xdir", kind=EntryKind.TREE, **defaults)  # type: ignore
    assert root_dir > root_file


def test_manifest_sorting(tmpdir: py.path.local) -> None:

    tmpdir.join("dir1.0/dir2.0/file.txt").write("somedata", ensure=True)
    tmpdir.join("dir1.0/dir2.1/file.txt").write("someotherdata", ensure=True)
    tmpdir.join("dir1.0/file.txt").write("thebestdata", ensure=True)
    tmpdir.join("dir2.0/file.txt").write("evenmoredata", ensure=True)
    tmpdir.join("a_file.txt").write("rootdata", ensure=True)
    tmpdir.join("z_file.txt").write("rootdata", ensure=True)

    manifest = MutableManifest(tmpdir.strpath)
    compute_entry(tmpdir.strpath, append_to=manifest)

    items = list(manifest._by_path.items())
    random.shuffle(items)
    manifest._by_path = OrderedDict(items)

    manifest.sort()
    actual = list(manifest._by_path.keys())
    expected = [
        "/",
        "/a_file.txt",
        "/z_file.txt",
        "/dir1.0",
        "/dir1.0/file.txt",
        "/dir1.0/dir2.0",
        "/dir1.0/dir2.0/file.txt",
        "/dir1.0/dir2.1",
        "/dir1.0/dir2.1/file.txt",
        "/dir2.0",
        "/dir2.0/file.txt",
    ]
    assert actual == expected


def test_later_manifests(tmpdir: py.path.local) -> None:

    a_dir = tmpdir.join("a").ensure(dir=True)
    a_dir.join("a.txt").write("a", ensure=True)
    a_dir.join("both.txt").write("a", ensure=True)
    a = compute_manifest(a_dir.strpath)

    b_dir = tmpdir.join("b").ensure(dir=True)
    b_dir.join("b.txt").write("b", ensure=True)
    b_dir.join("both.txt").write("b", ensure=True)
    b = compute_manifest(b_dir.strpath)

    both_dir = tmpdir.join("both").ensure(dir=True)
    both_dir.join("a.txt").write("a", ensure=True)
    both_dir.join("b.txt").write("b", ensure=True)
    both_dir.join("both.txt").write("b", ensure=True)
    both = compute_manifest(both_dir.strpath)

    actual = layer_manifests(a, b)

    assert actual.digest == both.digest


def test_layer_manifests_removal() -> None:

    a = MutableManifest("/")
    a.add_entry(
        "/a_only", Entry(kind=EntryKind.BLOB, mode=0o000777, name="a_only", object="")
    )

    b = MutableManifest("/")
    b.add_entry(
        "/a_only", Entry(kind=EntryKind.MASK, mode=0o020000, name="a_only", object="")
    )

    actual = layer_manifests(a.finalize(), b.finalize())
    assert actual.paths == ("/", "/a_only")
    entry = actual.get_path("/a_only")
    assert entry is not None
    assert entry.kind is EntryKind.MASK
