import uuid

import pytest

import spenv
from spenv.storage._package import _ensure_package


@pytest.fixture
def tmprepo(tmpdir):

    return spenv.storage.Repository(tmpdir.join("tmprepo").strpath)


@pytest.fixture
def mkpkg(tmpdir):
    def mkpkg() -> spenv.storage.Package:

        return _ensure_package(tmpdir.join(uuid.uuid1().hex).strpath)

    return mkpkg
