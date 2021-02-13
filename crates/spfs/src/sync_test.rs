import pytest
import py.path

from . import storage, tracking, graph, encoding
from ._config import Config
from ._sync import sync_layer, push_ref, sync_ref


def test_push_ref_unknown() -> None:

    with pytest.raises(graph.UnknownReferenceError):
        push_ref("--test-unknown--", "origin")

    with pytest.raises(graph.UnknownReferenceError):
        push_ref(str(encoding.NULL_DIGEST), "origin")


@pytest.mark.timeout(5)
def test_push_ref(config: Config, tmpdir: py.path.local) -> None:

    src_dir = tmpdir.join("source")
    src_dir.join("dir/file.txt").write("hello", ensure=True)
    src_dir.join("dir2/otherfile.txt").write("hello2", ensure=True)
    src_dir.join("dir//dir/dir/file.txt").write("hello, world", ensure=True)

    local = config.get_repository()
    remote = config.get_remote("origin")
    manifest = local.commit_dir(src_dir.strpath)
    layer = local.create_layer(storage.Manifest(manifest))
    local.tags.push_tag("testing", layer.digest())

    push_ref("testing", "origin")

    assert remote.read_ref("testing")
    assert remote.has_layer(layer.digest())

    push_ref("testing", "origin")


@pytest.mark.timeout(5)
def test_sync_ref(tmpdir: py.path.local) -> None:

    src_dir = tmpdir.join("source")
    src_dir.join("dir/file.txt").write("hello", ensure=True)
    src_dir.join("dir2/otherfile.txt").write("hello2", ensure=True)
    src_dir.join("dir//dir/dir/file.txt").write("hello, world", ensure=True)

    repo_a = storage.fs.FSRepository(tmpdir.join("repo_a").strpath, create=True)
    repo_b = storage.fs.FSRepository(tmpdir.join("repo_b").strpath, create=True)

    manifest = repo_a.commit_dir(src_dir.strpath)
    layer = repo_a.create_layer(storage.Manifest(manifest))
    platform = repo_a.create_platform([layer.digest()])
    repo_a.tags.push_tag("testing", platform.digest())

    sync_ref("testing", repo_a, repo_b)

    assert repo_b.read_ref("testing")
    assert repo_b.has_platform(platform.digest())
    assert repo_b.has_layer(layer.digest())

    tmpdir.join("repo_a").remove()
    tmpdir.join("repo_a").ensure(dir=1)
    sync_ref("testing", repo_b, repo_a)

    assert repo_a.read_ref("testing")
    assert repo_a.has_layer(layer.digest())


def test_sync_through_tar(tmpdir: py.path.local) -> None:

    src_dir = tmpdir.join("source")
    src_dir.join("dir/file.txt").write("hello", ensure=True)
    src_dir.join("dir2/otherfile.txt").write("hello2", ensure=True)
    src_dir.join("dir//dir/dir/file.txt").write("hello, world", ensure=True)

    repo_a = storage.fs.FSRepository(tmpdir.join("repo_a").strpath, create=True)
    repo_tar = storage.tar.TarRepository(tmpdir.join("repo.tar").strpath)
    repo_b = storage.fs.FSRepository(tmpdir.join("repo_b").strpath, create=True)

    manifest = repo_a.commit_dir(src_dir.strpath)
    layer = repo_a.create_layer(storage.Manifest(manifest))
    platform = repo_a.create_platform([layer.digest()])
    repo_a.tags.push_tag("testing", platform.digest())

    sync_ref("testing", repo_a, repo_tar)
    repo_tar = storage.tar.TarRepository(tmpdir.join("repo.tar").strpath)
    sync_ref("testing", repo_tar, repo_b)

    assert repo_b.read_ref("testing")
    assert repo_b.has_layer(layer.digest())
