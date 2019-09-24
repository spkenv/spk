from typing import Callable
import uuid
import logging

import pytest
import py.path
import structlog

import spenv
from spenv.storage.fs._layer import _ensure_layer

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
def tmprepo(tmpdir: py.path.local) -> spenv.storage.fs.Repository:

    return spenv.storage.fs.Repository(tmpdir.join("tmprepo").strpath)


@pytest.fixture
def mklayer(tmpdir: py.path.local) -> Callable[[], spenv.storage.fs.Layer]:
    def mklayer() -> spenv.storage.fs.Layer:

        return _ensure_layer(tmpdir.join(uuid.uuid1().hex).strpath)

    return mklayer
