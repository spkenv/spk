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


@pytest.fixture(autouse=True)
def spfs_editable(tmpspfs: None) -> None:

    try:
        runtime = spkrs.active_runtime()
    except spkrs.NoRuntimeError:
        pytest.fail("Tests must be run in an spfs environment")
        return

    runtime.reset_stack()
    runtime.set_editable(True)
    spkrs.remount_runtime(runtime)
    runtime.reset()
