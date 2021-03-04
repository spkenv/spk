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
spkrs.configure_logging(0)


@pytest.fixture
def tmprepo(tmpspfs: spkrs.SpFSRepository) -> spk.storage.SpFSRepository:

    from spk import storage

    return storage.SpFSRepository(tmpspfs)


@pytest.fixture(autouse=True)
def spfs_editable(tmpspfs: None) -> None:

    try:
        spkrs.reconfigure_runtime(editable=True, reset=["*"], stack=[])
        yield
        spkrs.reconfigure_runtime(editable=True, reset=["*"], stack=[])
    except Exception as e:
        pytest.fail("Tests must be run in an spfs environment: " + str(e))
        return


@pytest.fixture(autouse=True)
def tmpspfs(tmpdir: py.path.local, monkeypatch: Any) -> spkrs.SpFSRepository:

    root = tmpdir.join("spfs_repo").strpath
    origin_root = tmpdir.join("spfs_origin").strpath
    monkeypatch.setenv("SPFS_STORAGE_ROOT", root)
    monkeypatch.setenv("SPFS_REMOTE_ORIGIN_ADDRESS", "file:" + origin_root)
    for path in [root, origin_root]:
        r = py.path.local(path)
        r.join("renders").ensure(dir=True)
        r.join("objects").ensure(dir=True)
        r.join("payloads").ensure(dir=True)
        r.join("tags").ensure(dir=True)
    spkrs.SpFSRepository("file:" + origin_root)
    return spkrs.SpFSRepository("file:" + root)
