from typing import Callable
import uuid

import pytest
import py.path

import spenv
from spenv.storage._layer import _ensure_layer


@pytest.fixture
def tmprepo(tmpdir: py.path.local) -> spenv.storage.FileRepository:

    return spenv.storage.FileRepository(tmpdir.join("tmprepo").strpath)


@pytest.fixture
def mklayer(tmpdir: py.path.local) -> Callable[[], spenv.storage.Layer]:
    def mklayer() -> spenv.storage.Layer:

        return _ensure_layer(tmpdir.join(uuid.uuid1().hex).strpath)

    return mklayer
