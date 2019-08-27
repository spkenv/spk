import pytest

import spenv


@pytest.fixture
def tmprepo(tmpdir):

    return spenv.storage.Repository(tmpdir.join("tmprepo").strpath)
