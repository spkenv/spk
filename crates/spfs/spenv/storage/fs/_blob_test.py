import py.path

from ... import tracking
from ._blob import BlobStorage


def test_commit_dir(tmpdir: py.path.local) -> None:

    storage = BlobStorage(tmpdir.join("storage").strpath)

    src_dir = tmpdir.join("source")
    src_dir.join("dir1.0/dir2.0/file.txt").write("somedata", ensure=True)
    src_dir.join("dir1.0/dir2.1/file.txt").write("someotherdata", ensure=True)
    src_dir.join("dir2.0/file.txt").write("evenmoredata", ensure=True)
    src_dir.join("file.txt").write("rootdata", ensure=True)

    manifest = storage.commit_dir(src_dir.strpath)
    assert py.path.local(manifest.root).exists()

    manifest2 = storage.commit_dir(src_dir.strpath)
    assert manifest.digest == manifest2.digest


def test_render_manifest(tmpdir: py.path.local) -> None:

    storage = BlobStorage(tmpdir.join("storage").strpath)

    src_dir = tmpdir.join("source")
    src_dir.join("dir1.0/dir2.0/file.txt").write("somedata", ensure=True)
    src_dir.join("dir1.0/dir2.1/file.txt").write("someotherdata", ensure=True)
    src_dir.join("dir2.0/file.txt").write("evenmoredata", ensure=True)
    src_dir.join("file.txt").write("rootdata", ensure=True)

    expected = tracking.compute_manifest(src_dir.strpath)

    for path, entry in expected.walk_abs():
        if entry.kind is tracking.EntryKind.BLOB:
            with open(path, "rb") as f:
                storage.write_blob(f)

    rendered_path = storage.render_manifest(expected)
    actual = tracking.compute_manifest(rendered_path)
    assert actual.digest == expected.digest
