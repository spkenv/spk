import pytest
import py.path

from . import storage, tracking
from ._config import Config
from ._sync import push_layer, push_ref


def test_push_ref_unknown(tmprepo: storage.Repository) -> None:

    with pytest.raises(ValueError):
        push_ref("--test-unknown--", tmprepo)


def test_push_layer_empty(config: Config, tmprepo: storage.fs.Repository) -> None:

    manifest = tracking.compute_manifest(tmprepo.root)
    layer = storage.Layer(manifest=manifest)
    push_layer(layer, tmprepo)


def test_push_layer(config: Config, tmpdir: py.path.local) -> None:

    src_dir = tmpdir.join("source")
    src_dir.join("dir/file.txt").write("hello", ensure=True)
    src_dir.join("dir2/otherfile.txt").write("hello2", ensure=True)
    src_dir.join("dir//dir/dir/file.txt").write("hello, world", ensure=True)

    repo = config.get_repository()
    manifest = repo.blobs.commit_dir(src_dir.strpath)
    layer = storage.Layer(manifest=manifest)

    remote = config.get_remote("origin")
    push_layer(layer, remote)

    assert remote.has_layer(layer.digest)

    push_layer(layer, remote)
