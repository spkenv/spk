from typing import Any

import py.path
import pytest
import spkrs

from ._repository import Repository
from ._spfs import SpFSRepository
from ._mem import MemRepository


def pytest_generate_tests(metafunc: Any) -> None:
    if "repo" in metafunc.fixturenames:
        metafunc.parametrize("repo", [SpFSRepository, MemRepository], indirect=True)


@pytest.fixture
def repo(request: Any, tmpdir: py.path.local) -> Repository:

    if request.param is MemRepository:
        return MemRepository()
    if request.param is SpFSRepository:
        return SpFSRepository(
            spkrs.storage.fs.FSRepository(tmpdir.join("repo").strpath, create=True)
        )

    raise NotImplementedError(
        "Unknown repository type to be tested: " + str(request.param)
    )
