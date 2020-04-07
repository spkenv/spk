from typing import Callable
import io

import py.path

from .. import tracking
from .fs import FSDatabase
from ._manifest import Manifest, ManifestStorage


def test_read_write_manifest(tmpdir: py.path.local) -> None:

    storage = ManifestStorage(FSDatabase(tmpdir.join("storage").strpath))

    tmpdir.join("file.txt").ensure()
    manifest = Manifest(tracking.compute_manifest(tmpdir.strpath))
    storage._db.write_object(manifest)

    tmpdir.join("file.txt").write("newrootdata", ensure=True)
    manifest2 = Manifest(tracking.compute_manifest(tmpdir.strpath))
    storage._db.write_object(manifest)

    assert manifest.digest() in list(storage._db.iter_digests())


def test_manifest_encoding_speed(benchmark: Callable) -> None:

    repo_manifest = Manifest(tracking.compute_manifest("."))
    stream = io.BytesIO()

    @benchmark
    def encode_decode() -> None:
        stream.seek(0)
        repo_manifest.encode(stream)
        stream.seek(0)
        Manifest.decode(stream)
