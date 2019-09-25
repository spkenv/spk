import os

import py.path
import pytest

from ... import tracking
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
    assert layer._root == tmpdir.join("--id--")


def test_commit_manifest(tmpdir: py.path.local) -> None:

    storage = LayerStorage(tmpdir.join("storage").strpath)

    tmpdir.join("file.txt").ensure()
    manifest = tracking.compute_manifest(tmpdir.strpath)

    layer = storage.commit_manifest(manifest)
    assert py.path.local(layer.rootdir).exists()

    layer2 = storage.commit_manifest(manifest)
    assert layer.digest == layer2.digest

    tmpdir.join("file.txt").write("newrootdata", ensure=True)
    manifest = tracking.compute_manifest(tmpdir.strpath)
    layer3 = storage.commit_manifest(manifest)

    assert layer3.digest != layer2.digest
