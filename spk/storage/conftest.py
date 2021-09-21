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
        return MemRepository()
    if request.param is SpFSRepository:
        repo = tmpdir.join("repo")
        repo.join("renders").ensure_dir()
        repo.join("objects").ensure_dir()
        repo.join("payloads").ensure_dir()
        repo.join("tags").ensure_dir()
        return SpFSRepository(spkrs.storage.SpFSRepository("file:" + repo.strpath))

    raise NotImplementedError(
        "Unknown repository type to be tested: " + str(request.param)
    )
