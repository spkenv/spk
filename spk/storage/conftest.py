# Copyright (c) 2021 Sony Pictures Imageworks, et al.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/imageworks/spk

from typing import Any

import py.path
import pytest
import spkrs

from ._repository import Repository


def pytest_generate_tests(metafunc: Any) -> None:
    if "repo" in metafunc.fixturenames:
        metafunc.parametrize(
            "repo",
            [spkrs.storage.open_spfs_repository, spkrs.storage.mem_repository],
            indirect=True,
        )


@pytest.fixture
def repo(tmpspfs: None, request: Any, tmpdir: py.path.local) -> Repository:

    if request.param is spkrs.storage.mem_repository:
        return request.param()  # type: ignore
    if request.param is spkrs.storage.open_spfs_repository:
        repo = tmpdir.join("repo").ensure_dir()
        return request.param(repo.strpath, create=True)  # type: ignore

    raise NotImplementedError(
        "Unknown repository type to be tested: " + str(request.param)
    )
