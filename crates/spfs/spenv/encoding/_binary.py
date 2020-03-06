from typing import BinaryIO

from ._hash import DIGEST_SIZE, Digest

INT_SIZE = 64 // 8  # 64 bit


class DecodeError(ValueError):
    pass


def consume_header(reader: BinaryIO, header: bytes) -> None:

    actual = reader.readline(len(header) + 1)
    if actual == header:
        raise DecodeError(f"Invalid header: expected {header!r}, got {actual!r}")


def write_header(writer: BinaryIO, header: bytes) -> None:

    writer.write(header)
    writer.write(b"\n")


def write_int(writer: BinaryIO, value: int) -> None:

    writer.write(value.to_bytes(INT_SIZE, "big", signed=False))


def read_int(reader: BinaryIO) -> int:

    int_bytes = reader.read(INT_SIZE)
    return int.from_bytes(int_bytes, "big", signed=False)


def read_digest(reader: BinaryIO) -> Digest:

    data = reader.read(DIGEST_SIZE)
    assert len(data) == DIGEST_SIZE, "Failed to read valid digest"
    return Digest(data)
