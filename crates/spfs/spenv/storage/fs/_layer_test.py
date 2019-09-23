import os

import py.path
import pytest

from ._layer import Layer, LayerStorage, _ensure_layer


def test_layer_properties(tmpdir: py.path.local) -> None:

    layer = Layer(tmpdir.strpath)
    assert tmpdir.bestrelpath(layer.rootdir) == "."
    assert os.path.basename(layer.diffdir) == Layer._diffdir
    assert os.path.basename(layer.metadir) == Layer._metadir


def test_list_no_layers(tmpdir: py.path.local) -> None:

    storage = LayerStorage(tmpdir.strpath)
    assert storage.list_layers() == []


def test_list_no_storage() -> None:

    storage = LayerStorage("/tmp/doesnotexist  ")
    assert storage.list_layers() == []


def test_remove_no_layer(tmpdir: py.path.local) -> None:

    storage = LayerStorage(tmpdir.strpath)
    with pytest.raises(ValueError):
        storage.remove_layer("noexist")


def test_remove_layer(tmpdir: py.path.local) -> None:

    storage = LayerStorage(tmpdir.strpath)
    _ensure_layer(tmpdir.join("layer").ensure(dir=True))
    storage.remove_layer("layer")
    assert not tmpdir.join("layer").exists()


def test_read_layer_noexist(tmpdir: py.path.local) -> None:

    storage = LayerStorage(tmpdir.strpath)
    with pytest.raises(ValueError):
        storage.read_layer("noexist")


def test_read_layer(tmpdir: py.path.local) -> None:

    storage = LayerStorage(tmpdir.strpath)
    storage._ensure_layer("--id--")
    layer = storage.read_layer("--id--")
    assert isinstance(layer, Layer)
    assert layer.ref == "--id--"


def test_commit_dir(tmpdir: py.path.local) -> None:

    storage = LayerStorage(tmpdir.join("storage").strpath)

    src_dir = tmpdir.join("source")
    src_dir.join("dir1.0/dir2.0/file.txt").write("somedata", ensure=True)
    src_dir.join("dir1.0/dir2.1/file.txt").write("someotherdata", ensure=True)
    src_dir.join("dir2.0/file.txt").write("evenmoredata", ensure=True)
    src_dir.join("file.txt").write("rootdata", ensure=True)

    layer = storage.commit_dir(src_dir.strpath)
    assert py.path.local(layer.rootdir).exists()

    layer2 = storage.commit_dir(src_dir.strpath)
    assert layer.ref == layer2.ref

    src_dir.join("file.txt").write("newrootdata", ensure=True)
    layer3 = storage.commit_dir(src_dir.strpath)

    assert layer3.ref != layer2.ref
