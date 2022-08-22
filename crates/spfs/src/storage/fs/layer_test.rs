// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

import os

import py.path
import pytest

from ... import tracking, encoding
from .. import Layer, LayerStorage, fs


def test_read_layer_noexist(tmpdir: py.path.local) -> None:

    storage = LayerStorage(fs.FSDatabase(tmpdir.strpath))
    with pytest.raises(ValueError):
        storage.read_layer(encoding.EMPTY_DIGEST)
