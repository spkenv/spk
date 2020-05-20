import py.path
import pytest
import spfs

import structlog

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
def tmpspfs(tmpdir: py.path.local) -> spfs.storage.fs.FSRepository:

    root = tmpdir.join("spfs_repo").strpath
    config = spfs.get_config()
    config.clear()
    config.add_section("storage")
    config.set("storage", "root", root)
    return spfs.storage.fs.FSRepository(root, create=True)
