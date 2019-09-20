import uuid

import pytest

import spenv
from spenv.storage._layer import _ensure_layer


@pytest.fixture
def tmprepo(tmpdir):

    return spenv.storage.Repository(tmpdir.join("tmprepo").strpath)


@pytest.fixture
def mklayer(tmpdir):
    def mklayer() -> spenv.storage.Layer:

        return _ensure_layer(tmpdir.join(uuid.uuid1().hex).strpath)

    return mklayer
