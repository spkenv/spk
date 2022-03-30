# Copyright (c) 2021 Sony Pictures Imageworks, et al.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/imageworks/spk

from typing import Any
import py.path
import pytest
import spkrs
import logging

import structlog
import spk


logging.getLogger("").setLevel(logging.DEBUG)
structlog.configure(
    processors=[
        structlog.stdlib.add_log_level,
        structlog.stdlib.PositionalArgumentsFormatter(),
        structlog.processors.StackInfoRenderer(),
        structlog.processors.format_exc_info,
        structlog.dev.ConsoleRenderer(),
    ],
    logger_factory=structlog.stdlib.LoggerFactory(),
    wrapper_class=structlog.stdlib.BoundLogger,
)
spkrs.configure_logging(2)


@pytest.fixture
def tmprepo(tmpspfs: spkrs.storage.Repository) -> spkrs.storage.Repository:

    return tmpspfs


@pytest.fixture(autouse=True)
def spfs_editable(tmpspfs: spkrs.storage.Repository) -> None:

    try:
        spkrs.reconfigure_runtime(editable=True, reset=["*"], stack=[])
        yield
        spkrs.reconfigure_runtime(editable=True, reset=["*"], stack=[])
    except Exception as e:
        pytest.fail("Tests must be run in an spfs environment: " + str(e))
        return


@pytest.fixture(autouse=True)
def tmpspfs(tmpdir: py.path.local, monkeypatch: Any) -> spkrs.storage.Repository:

    root = tmpdir.join("spfs_repo").ensure(dir=1).strpath
    origin_root = tmpdir.join("spfs_origin").ensure(dir=1).strpath
    monkeypatch.setenv("SPFS_STORAGE_ROOT", root)
    # we rely on an outer runtime being created and it needs to still be found
    monkeypatch.setenv("SPFS_STORAGE_RUNTIMES", "/tmp/spfs-runtimes")
    monkeypatch.setenv("SPFS_REMOTE_ORIGIN_ADDRESS", "file:" + origin_root)
    spkrs.reload_config()
    spkrs.storage.open_spfs_repository(origin_root, create=True)
    return spkrs.storage.open_spfs_repository(root, create=True)
