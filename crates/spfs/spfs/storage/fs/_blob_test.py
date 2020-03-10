import stat

import py.path

from ... import tracking
from ._database import FSPayloadStorage
from ._blob import FSManifestViewer, _copy_manifest, _was_render_completed


def test_render_manifest(tmpdir: py.path.local) -> None:

    storage = FSPayloadStorage(tmpdir.join("storage").strpath)
    viewer = FSManifestViewer(tmpdir.join("renders"), storage)

    src_dir = tmpdir.join("source")
    src_dir.join("dir1.0/dir2.0/file.txt").write("somedata", ensure=True)
    src_dir.join("dir1.0/dir2.1/file.txt").write("someotherdata", ensure=True)
    src_dir.join("dir2.0/file.txt").write("evenmoredata", ensure=True)
    src_dir.join("file.txt").write("rootdata", ensure=True)

    expected = tracking.compute_manifest(src_dir.strpath)

    for path, entry in expected.walk_abs(src_dir.strpath):
        if entry.kind is tracking.EntryKind.BLOB:
            with open(path, "rb") as f:
                storage.write_payload(f)

    rendered_path = viewer.render_manifest(expected)
    actual = tracking.compute_manifest(rendered_path)
    assert actual.digest() == expected.digest()


def test_copy_manfest(tmpdir: py.path.local) -> None:

    src_dir = tmpdir.join("source")
    src_dir.join("dir1.0/dir2.0/file.txt").write("somedata", ensure=True)
    src_dir.join("dir1.0/dir2.1/file.txt").write("someotherdata", ensure=True)
    src_dir.join("dir2.0/file.txt").write("evenmoredata", ensure=True)
    src_dir.join("dir2.0/file2.txt").mksymlinkto("file.txt")
    src_dir.join("dir2.0/abssrc").mksymlinkto(src_dir.strpath)
    src_dir.join("dir2.0").chmod(0o555)
    src_dir.join("file.txt").write("rootdata", ensure=True)
    src_dir.join("file.txt").chmod(0o400)

    expected = tracking.compute_manifest(src_dir.strpath)

    dst_dir = tmpdir.join("dest")
    _copy_manifest(expected, src_dir.strpath, dst_dir.strpath)

    actual = tracking.compute_manifest(dst_dir.strpath)

    assert actual.digest() == expected.digest()
