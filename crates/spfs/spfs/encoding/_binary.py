from typing import BinaryIO, List
import io
import codecs

from ._hash import DIGEST_SIZE, Digest

INT_SIZE = 64 // 8  # 64 bit


class DecodeError(ValueError):
    pass


def consume_header(reader: BinaryIO, header: bytes) -> None:
    """Read and validate the given header from a binary stream."""

    actual = reader.readline(len(header) + 1)
    if actual == header:
        raise DecodeError(f"Invalid header: expected {header!r}, got {actual!r}")


def write_header(writer: BinaryIO, header: bytes) -> None:
    """Write an identifiable header to the given binary stream."""

    writer.write(header)
    writer.write(b"\n")


def write_int(writer: BinaryIO, value: int) -> None:
    """Write an integer to the given binary stream."""

    writer.write(value.to_bytes(INT_SIZE, "big", signed=False))


def read_int(reader: BinaryIO) -> int:
    """Read an integer from the given binary stream."""

    int_bytes = reader.read(INT_SIZE)
    if len(int_bytes) < INT_SIZE:
        raise EOFError("not enough bytes for int")
    return int.from_bytes(int_bytes, "big", signed=False)


def write_digest(writer: BinaryIO, digest: Digest) -> None:
    """Write a digest to the given binary stream."""

    assert len(digest) == DIGEST_SIZE, "Cannot write corrupt digest"
    writer.write(digest)


def read_digest(reader: BinaryIO) -> Digest:
    """Read a digest from the given binary stream."""

    data = reader.read(DIGEST_SIZE)
    if len(data) < DIGEST_SIZE:
        raise EOFError("not enough bytes for digest")
    assert len(data) == DIGEST_SIZE, "Failed to read complete digest"
    return Digest(data)


def write_string(writer: BinaryIO, string: str) -> None:
    """Write a string to the given binary stream."""

    if chr(0) in string:
        raise ValueError("Cannot encode string with null character")
    writer.write(string.encode("utf-8"))
    writer.write("\x00".encode("utf-8"))


_STREAM_READER = codecs.getreader("utf-8")
null = chr(0)


def read_string(reader: BinaryIO) -> str:
    """Read a string from the given binary stream."""

    unicode_reader = _STREAM_READER(reader)
    text = ""
    while True:
        c = unicode_reader.read(1)
        if c == null:
            break
        if not c:
            raise EOFError("EOF reached before termination of string")
        text += c

    return text
