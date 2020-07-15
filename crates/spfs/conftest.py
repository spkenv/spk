import os
import logging

import pytest
import py.path
import structlog

import spfs

logging.basicConfig()
logging.getLogger().setLevel(logging.DEBUG)
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
def tmprepo(tmpdir: py.path.local) -> spfs.storage.fs.FSRepository:

    root = tmpdir.join("tmprepo").ensure(dir=True)
    return spfs.storage.fs.FSRepository(root.strpath)


@pytest.fixture(scope="session")
def testdata() -> py.path.local:

    here = os.path.dirname(__file__)
    return py.path.local(here).join("testdata")


@pytest.fixture(autouse=True)
def config(tmpdir: py.path.local) -> spfs.Config:

    tmpdir.join("remote_origin").ensure(dir=1)
    spfs._config._CONFIG = spfs.Config()
    spfs._config._CONFIG.read_string(
        f"""
[storage]
root = {tmpdir.join('storage_root').strpath}

[remote.origin]
address = file://{tmpdir.join('remote_origin').strpath}
"""
    )
    return spfs.get_config()


@pytest.fixture
def with_install() -> None:
    if "CI" in os.environ:
        pytest.skip("Cannot test against rpm install in CI")
    print(
        "This test requires a privileged install of spfs-enter, and may fail otherwise"
    )


assert (
    "SPFS_RUNTIME" not in os.environ
), "Already in an SpFS runtime -- not good for testing"
