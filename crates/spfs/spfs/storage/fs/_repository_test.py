import pytest
import py.path

from ._repository import Repository, ensure_repository


def test_find_aliases(tmpdir: py.path.local) -> None:

    repo = ensure_repository(tmpdir.strpath)
    with pytest.raises(ValueError):
        repo.find_aliases("not-existant")

    tmpdir.join("data", "file.txt").ensure()
    manifest = repo.blobs.commit_dir(tmpdir.join("data").strpath)
    layer = repo.layers.commit_manifest(manifest)
    repo.tags.push_tag("test-tag", layer.digest)

    assert repo.find_aliases(layer.digest) == ["test-tag"]
    assert repo.find_aliases("test-tag") == [layer.digest]
