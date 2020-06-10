import py.path
import pytest
import spfs
import logging

import structlog

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


@pytest.fixture
def tmprepo(tmpspfs: spfs.storage.fs.FSRepository) -> spfs.storage.fs.FSRepository:

    from spk import storage

    return storage.SpFSRepository(tmpspfs)


@pytest.fixture(autouse=True)
def spfs_editable(tmpspfs: None) -> None:

    try:
        runtime = spfs.active_runtime()
    except spfs.NoRuntimeError:
        pytest.fail("Tests must be run in an spfs environment")

    runtime.reset()
    runtime.set_editable(True)
    spfs.remount_runtime(runtime)


@pytest.fixture(autouse=True)
def tmpspfs(tmpdir: py._path.local.LocalPath) -> spfs.storage.fs.FSRepository:

    root = tmpdir.join("spfs_repo").strpath
    origin_root = tmpdir.join("spfs_origin").strpath
    config = spfs.get_config()
    config.clear()
    config.add_section("storage")
    config.add_section("remote.origin")
    config.set("storage", "root", root)
    config.set("remote.origin", "address", "file:" + origin_root)
    spfs.storage.fs.FSRepository(origin_root, create=True)
    return spfs.storage.fs.FSRepository(root, create=True)
