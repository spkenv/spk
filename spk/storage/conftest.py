# Copyright (c) 2021 Sony Pictures Imageworks, et al.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/imageworks/spk

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
def repo(tmpspfs: None, request: Any, tmpdir: py.path.local) -> Repository:

    if request.param is MemRepository:
        return spkrs.storage.mem_repository()
    if request.param is SpFSRepository:
        repo = tmpdir.join("repo").ensure_dir()
        return spkrs.storage.open_spfs_repository("file:" + repo.strpath, create=True)

    raise NotImplementedError(
        "Unknown repository type to be tested: " + str(request.param)
    )
