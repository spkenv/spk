import io
import random
import string

import pytest

from ._binary import (
    consume_header,
    write_header,
    read_int,
    write_int,
    read_digest,
    write_digest,
    read_string,
    write_string,
)


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


def random_word(length: int) -> str:
    chars = string.ascii_letters + "01234567890"
    return "".join(random.choice(chars) for i in range(length))


@pytest.mark.parametrize("_", range(10))
def test_read_write_string(_: int) -> None:

    value = random_word(random.randint(256, 1024))
    postfix = random_word(random.randint(256, 1024))

    stream = io.BytesIO()
    write_string(stream, value)
    write_string(stream, postfix)
    stream.write(b"postfix")
    stream.seek(0)
    assert read_string(stream) == value
    assert read_string(stream) == postfix
    assert stream.read() == b"postfix"
