import py.path

from ._manifest import Manifest, compute_manifest
from ._diff import Diff, DiffMode, compute_diff


def test_diff_str() -> None:

    assert Diff(DiffMode.added, "some_path")


def test_compute_diff_empty() -> None:

    a = Manifest("")
    b = Manifest("")

    assert compute_diff(a, b) == []


def test_compute_diff_same(tmpdir: py.path.local) -> None:

    tmpdir.join("dir/dir/file").write("data", ensure=True)
    tmpdir.join("dir/file").write("more", ensure=True)
    tmpdir.join("file").write("otherdata", ensure=True)

    manifest = compute_manifest(tmpdir.strpath)
    diffs = compute_diff(manifest, manifest)
    for diff in diffs:
        assert diff.mode is DiffMode.unchanged


def test_compute_diff_added(tmpdir: py.path.local) -> None:

    a_dir = tmpdir.join("a").ensure(dir=True)
    b_dir = tmpdir.join("b").ensure(dir=True)
    b_dir.join("dir/dir/file").write("data", ensure=True)

    a = compute_manifest(a_dir.strpath)
    b = compute_manifest(b_dir.strpath)
    actual = compute_diff(a, b)
    expected = [
        Diff(mode=DiffMode.changed, path="."),
        Diff(mode=DiffMode.added, path="./dir"),
        Diff(mode=DiffMode.added, path="./dir/dir"),
        Diff(mode=DiffMode.added, path="./dir/dir/file"),
    ]
    assert actual == expected


def test_compute_diff_removed(tmpdir: py.path.local) -> None:

    a_dir = tmpdir.join("a").ensure(dir=True)
    b_dir = tmpdir.join("b").ensure(dir=True)
    a_dir.join("dir/dir/file").write("data", ensure=True)

    a = compute_manifest(a_dir.strpath)
    b = compute_manifest(b_dir.strpath)
    actual = compute_diff(a, b)
    expected = [
        Diff(mode=DiffMode.changed, path="."),
        Diff(mode=DiffMode.removed, path="./dir"),
        Diff(mode=DiffMode.removed, path="./dir/dir"),
        Diff(mode=DiffMode.removed, path="./dir/dir/file"),
    ]
    assert actual == expected
