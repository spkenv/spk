from typing import Callable
from collections import OrderedDict
import io
import os
import json
import random

import py.path
import pytest

from .. import encoding, storage
from ._manifest import Manifest, Entry, EntryKind, compute_manifest, ManifestBuilder
from ._diff import compute_diff


def test_compute_manifest_determinism() -> None:

    first = compute_manifest("./spfs")
    second = compute_manifest("./spfs")
    assert storage.Manifest(first) == storage.Manifest(second)


def test_compute_manifest() -> None:

    root = os.path.abspath("./spfs")
    this = os.path.relpath(__file__, root)
    manifest = compute_manifest(root)
    assert manifest.get_path(this) is not None


def test_manifest_relative_paths(tmpdir: py.path.local) -> None:

    tmpdir.join("dir1.0/dir2.0/file.txt").write("somedata", ensure=True)
    tmpdir.join("dir1.0/dir2.1/file.txt").write("someotherdata", ensure=True)
    tmpdir.join("dir2.0/file.txt").write("evenmoredata", ensure=True)
    tmpdir.join("a_file.txt").write("rootdata", ensure=True)

    manifest = compute_manifest(tmpdir.strpath)
    assert manifest.list_dir("/"), "should be able to list root"
    assert manifest.get_path("/dir1.0/dir2.0/file.txt") is not None
    assert manifest.get_path("dir1.0/dir2.1/file.txt") is not None


def test_manifest_sorting(tmpdir: py.path.local) -> None:

    tmpdir = tmpdir.join("data")
    tmpdir.join("dir1.0/dir2.0/file.txt").write("somedata", ensure=True)
    tmpdir.join("dir1.0/dir2.1/file.txt").write("someotherdata", ensure=True)
    tmpdir.join("dir1.0/file.txt").write("thebestdata", ensure=True)
    tmpdir.join("dir2.0/file.txt").write("evenmoredata", ensure=True)
    tmpdir.join("a_file.txt").write("rootdata", ensure=True)
    tmpdir.join("z_file.txt").write("rootdata", ensure=True)

    manifest = Manifest()
    builder = ManifestBuilder()
    builder._compute_tree_node(tmpdir.strpath, manifest.root)

    actual = list(p for p, _ in manifest.walk())
    expected = [
        "/dir1.0",
        "/dir1.0/dir2.0",
        "/dir1.0/dir2.0/file.txt",
        "/dir1.0/dir2.1",
        "/dir1.0/dir2.1/file.txt",
        "/dir1.0/file.txt",
        "/dir2.0",
        "/dir2.0/file.txt",
        "/a_file.txt",
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

    a.update(b)

    assert storage.Manifest(a) == storage.Manifest(both)


def test_layer_manifests_removal() -> None:

    a = Manifest()
    a.mkfile("a_only")

    b = Manifest()
    node = b.mkfile("a_only")
    node.kind = EntryKind.MASK

    c = Manifest()
    c.update(a)
    assert c.get_path("/a_only").kind is EntryKind.BLOB
    c.update(b)
    assert c.get_path("/a_only").kind is EntryKind.MASK


repo_manifest = compute_manifest(".")


def test_manifest_layering_speed(benchmark: Callable) -> None:

    benchmark(repo_manifest.update, repo_manifest)
