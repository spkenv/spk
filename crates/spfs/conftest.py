from typing import Callable
import uuid

import pytest
import py.path

import spenv
from spenv.storage.fs._layer import _ensure_layer


@pytest.fixture
def tmprepo(tmpdir: py.path.local) -> spenv.storage.fs.Repository:

    return spenv.storage.fs.Repository(tmpdir.join("tmprepo").strpath)


@pytest.fixture
def mklayer(tmpdir: py.path.local) -> Callable[[], spenv.storage.fs.Layer]:
    def mklayer() -> spenv.storage.fs.Layer:

        return _ensure_layer(tmpdir.join(uuid.uuid1().hex).strpath)

    return mklayer
