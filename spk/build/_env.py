from typing import Iterator
import signal
from contextlib import contextmanager


@contextmanager
def deferred_signals() -> Iterator[None]:

    # do not react to os signals while the subprocess is running,
    # these should be handled by the underlying process instead
    signal.signal(signal.SIGINT, lambda *_: None)
    signal.signal(signal.SIGTERM, lambda *_: None)
    try:
        yield None
    finally:
        signal.signal(signal.SIGINT, signal.SIG_DFL)
        signal.signal(signal.SIGTERM, signal.SIG_DFL)
