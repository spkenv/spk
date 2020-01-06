from collections import OrderedDict
import os
import json
import random

import py.path
import pytest

from ._manifest import (
    ManifestBuilder,
    Manifest,
    Entry,
    EntryKind,
    compute_tree,
    compute_entry,
    compute_manifest,
    layer_manifests,
)
from ._diff import compute_diff


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
    with pytest.raises(FileNotFoundError):
        # should be no entry for root - as there is not enough info
        # about the root to form an entry (missing mode and name)
        manifest.get_path("/")
    assert manifest.get_path("/dir1.0/dir2.0/file.txt") is not None
    assert manifest.get_path("dir1.0/dir2.1/file.txt") is not None


def test_manifest_sorting(tmpdir: py.path.local) -> None:

    tmpdir.join("dir1.0/dir2.0/file.txt").write("somedata", ensure=True)
    tmpdir.join("dir1.0/dir2.1/file.txt").write("someotherdata", ensure=True)
    tmpdir.join("dir1.0/file.txt").write("thebestdata", ensure=True)
    tmpdir.join("dir2.0/file.txt").write("evenmoredata", ensure=True)
    tmpdir.join("a_file.txt").write("rootdata", ensure=True)
    tmpdir.join("z_file.txt").write("rootdata", ensure=True)

    manifest = ManifestBuilder(tmpdir.strpath)
    compute_entry(tmpdir.strpath, append_to=manifest)

    final = manifest.finalize()
    actual = list(p for p, _ in final.walk())
    expected = [
        "/a_file.txt",
        "/dir1.0",
        "/dir1.0/dir2.0",
        "/dir1.0/dir2.0/file.txt",
        "/dir1.0/dir2.1",
        "/dir1.0/dir2.1/file.txt",
        "/dir1.0/file.txt",
        "/dir2.0",
        "/dir2.0/file.txt",
        "/z_file.txt",
    ]
    assert actual == expected


def test_layer_manifests(tmpdir: py.path.local) -> None:

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

    a = ManifestBuilder("/")
    a.add_entry(
        "/a_only", Entry(kind=EntryKind.BLOB, mode=0o000777, name="a_only", object="")
    )

    b = ManifestBuilder("/")
    b.add_entry(
        "/a_only", Entry(kind=EntryKind.MASK, mode=0o020000, name="a_only", object="")
    )

    actual = layer_manifests(a.finalize(), b.finalize())
    entry = actual.get_path("/a_only")
    assert entry is not None
    assert entry.kind is EntryKind.MASK


def test_manifest_builder_remove_file() -> None:

    builder = ManifestBuilder("/")
    builder.add_entry(
        "/entry", Entry(kind=EntryKind.BLOB, mode=0o000777, name="entry", object="")
    )
    builder.remove_entry("/entry")

    manifest = builder.finalize()
    with pytest.raises(FileNotFoundError):
        manifest.get_path("/entry")
    with pytest.raises(FileNotFoundError):
        manifest.get_path("entry")


def test_manifest_builder_remove_dir() -> None:

    builder = ManifestBuilder("/")
    builder.add_entry(
        "/entry", Entry(kind=EntryKind.TREE, mode=0o000777, name="entry", object="")
    )
    builder.remove_entry("/entry")

    manifest = builder.finalize()
    with pytest.raises(FileNotFoundError):
        manifest.get_path("/entry")
    with pytest.raises(FileNotFoundError):
        manifest.get_path("entry")


def test_compatibility_0_12(testdata: py.path.local) -> None:
    """Ensure compatbility with pre 0.13 layer manifests.

    Older manifests can have corrupt tree information which
    was not an issue due to the use of duplicated data at runtime.
    """

    layer_file = testdata.join("compat", "layers", "0.12.14.json")
    with layer_file.open("r") as f:
        layer_data = json.load(f)

    manifest = Manifest.load_dict(layer_data["manifest"])

    for path, entry in manifest.walk():
        # the biggest issue that arises is with the internal
        # data being inconsistent and not being able to walk
        # the structure because of invalid digests being used
        assert path, "should be internally consistent"
        assert entry, "should be internally consistent"
