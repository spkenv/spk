import os

import py.path
import pytest

from ... import tracking
from ._layer import Layer, LayerStorage


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
    tmpdir.join("la/yer").ensure()
    storage.remove_layer("layer")
    assert not tmpdir.join("la/yer").exists()


def test_read_layer_noexist(tmpdir: py.path.local) -> None:

    storage = LayerStorage(tmpdir.strpath)
    with pytest.raises(ValueError):
        storage.read_layer("noexist")


def test_commit_manifest(tmpdir: py.path.local) -> None:

    storage = LayerStorage(tmpdir.join("storage").strpath)

    tmpdir.join("file.txt").ensure()
    manifest = tracking.compute_manifest(tmpdir.strpath)

    layer = storage.commit_manifest(manifest)
    assert tmpdir.join("storage", layer.digest[:2], layer.digest[2:]).exists()

    layer2 = storage.commit_manifest(manifest)
    assert layer.digest == layer2.digest

    tmpdir.join("file.txt").write("newrootdata", ensure=True)
    manifest = tracking.compute_manifest(tmpdir.strpath)
    layer3 = storage.commit_manifest(manifest)

    assert layer3.digest != layer2.digest
