from ._layer import Layer, LayerStorage, _ensure_layer


def test_list_no_layers(tmpdir):

    storage = LayerStorage(tmpdir.strpath)
    assert storage.list_layers() == []


def test_list_no_storage():

    storage = LayerStorage("/tmp/doesnotexist  ")
    assert storage.list_layers() == []


def test_remove_no_layer(tmpdir):

    storage = LayerStorage(tmpdir.strpath)
    storage.remove_layer("noexist")


def test_remove_layer(tmpdir):

    storage = LayerStorage(tmpdir.strpath)
    _ensure_layer(tmpdir.join("layer").ensure(dir=True))
    storage.remove_layer("layer")
    assert not tmpdir.join("layer").exists()
