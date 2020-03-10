from typing import NamedTuple, Any, Dict, BinaryIO
import enum
import hashlib

from .. import encoding


class EntryKind(enum.Enum):

    TREE = "tree"  # directory / node
    BLOB = "file"  # file / leaf
    MASK = "mask"  # removed entry / node or leaf


class Entry(encoding.Encodable):
    def __init__(
        self, object: encoding.Digest, kind: EntryKind, mode: int, size: int, name: str
    ) -> None:

        self.object = object
        self.kind = kind
        self.mode = mode
        self.size = size
        self.name = name
        super(Entry, self).__init__()

    def __str__(self) -> str:

        return f"{self.mode:06o} {self.kind.value} {self.name} {self.object.str()}"

    def __lt__(self, other: Any) -> bool:

        if not isinstance(other, Entry):
            raise TypeError(
                f"'<' not supported between '{type(self).__name__}' and '{type(other).__name__}'"
            )

        if self.kind is other.kind:
            return self.name < other.name

        return other.kind is EntryKind.TREE

    def __gt__(self, other: Any) -> bool:

        if not isinstance(other, Entry):
            raise TypeError(
                f"'>' not supported between '{type(self).__name__}' and '{type(other).__name__}'"
            )

        if self.kind is other.kind:
            return self.name > other.name

        return self.kind is EntryKind.TREE

    def encode(self, writer: BinaryIO) -> None:

        encoding.write_digest(writer, self.object)
        encoding.write_string(writer, self.kind.value)
        encoding.write_int(writer, self.mode)
        encoding.write_int(writer, self.size)
        encoding.write_string(writer, self.name)

    @classmethod
    def decode(cls, reader: BinaryIO) -> "Entry":

        return Entry(
            object=encoding.read_digest(reader),
            kind=EntryKind(encoding.read_string(reader)),
            mode=encoding.read_int(reader),
            size=encoding.read_int(reader),
            name=encoding.read_string(reader),
        )
