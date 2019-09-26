import pytest
import py.path

from . import storage, tracking
from ._config import Config
from ._sync import sync_layer, push_ref, sync_ref


def test_push_ref_unknown() -> None:

    with pytest.raises(ValueError):
        push_ref("--test-unknown--", "origin")


def test_push_ref(config: Config, tmpdir: py.path.local) -> None:

    src_dir = tmpdir.join("source")
    src_dir.join("dir/file.txt").write("hello", ensure=True)
    src_dir.join("dir2/otherfile.txt").write("hello2", ensure=True)
    src_dir.join("dir//dir/dir/file.txt").write("hello, world", ensure=True)

    local = config.get_repository()
    remote = config.get_remote("origin")
    manifest = local.blobs.commit_dir(src_dir.strpath)
    layer = local.layers.commit_manifest(manifest)
    local.write_tag("testing", layer.digest)

    push_ref("testing", "origin")

    assert remote.read_object("testing")
    assert remote.has_layer(layer.digest)

    push_ref("testing", "origin")


def test_push_pull_ref(tmpdir: py.path.local) -> None:

    src_dir = tmpdir.join("source")
    src_dir.join("dir/file.txt").write("hello", ensure=True)
    src_dir.join("dir2/otherfile.txt").write("hello2", ensure=True)
    src_dir.join("dir//dir/dir/file.txt").write("hello, world", ensure=True)

    repo_a = storage.fs.Repository(tmpdir.join("repo_a").strpath)
    repo_b = storage.fs.Repository(tmpdir.join("repo_b").strpath)

    manifest = repo_a.blobs.commit_dir(src_dir.strpath)
    layer = repo_a.layers.commit_manifest(manifest)
    repo_a.write_tag("testing", layer.digest)

    sync_ref("testing", repo_a, repo_b)

    assert repo_b.read_object("testing")
    assert repo_b.has_layer(layer.digest)

    tmpdir.join("repo_a").remove()
    sync_ref("testing", repo_b, repo_a)

    assert repo_a.read_object("testing")
    assert repo_a.has_layer(layer.digest)
