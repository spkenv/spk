import io

import pytest

from ._binary import consume_header, write_header, read_int, write_int


def test_consume_header() -> None:
    stream = io.BytesIO(b"HEADER\n")
    consume_header(stream, b"HEADER")

    assert stream.read() == b""


def test_write_read_header() -> None:
    header = b"HEADER"
    stream = io.BytesIO()
    write_header(stream, header)
    consume_header(stream, header)
    assert stream.read() == b""


@pytest.mark.parametrize("value", (0, 1, 45, 600))
def test_read_write_int(value: int) -> None:

    stream = io.BytesIO()
    write_int(stream, value)
    stream.write(b"postfix")
    stream.seek(0)
    assert read_int(stream) == value
    assert stream.read() == b"postfix"
