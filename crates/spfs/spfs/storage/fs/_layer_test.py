import os

import py.path
import pytest

from ... import tracking, encoding
from .. import Layer, LayerStorage, fs


def test_read_layer_noexist(tmpdir: py.path.local) -> None:

    storage = LayerStorage(fs.FileDB(tmpdir.strpath))
    with pytest.raises(ValueError):
        storage.read_layer(encoding.EMPTY_DIGEST)
