import py.path

from .. import tracking
from .fs import FileDB
from ._manifest import ManifestStorage


def test_read_write_manifest(tmpdir: py.path.local) -> None:

    storage = ManifestStorage(FileDB(tmpdir.join("storage").strpath))

    tmpdir.join("file.txt").ensure()
    manifest = tracking.compute_manifest(tmpdir.strpath)
    storage._db.write_object(manifest)

    tmpdir.join("file.txt").write("newrootdata", ensure=True)
    manifest2 = tracking.compute_manifest(tmpdir.strpath)
    storage._db.write_object(manifest)

    assert manifest.digest() in list(storage._db.iter_digests())
