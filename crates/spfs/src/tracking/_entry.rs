from typing import NamedTuple, Any, Dict, BinaryIO
import enum
import hashlib

from .. import encoding


class EntryKind(enum.Enum):

    TREE = "tree"  # directory / node
    BLOB = "file"  # file / leaf
    MASK = "mask"  # removed entry / node or leaf


class Entry(dict):

    __fields__ = ("kind", "object", "mode", "size")

    def __init__(
        self,
        kind: EntryKind = EntryKind.TREE,
        object: encoding.Digest = encoding.NULL_DIGEST,
        mode: int = 0o777,
        size: int = 0,
    ) -> None:
        self.kind = kind
        self.object = object
        self.mode = mode
        self.size = size

    def __str__(self) -> str:
        return repr(self)

    def __repr__(self) -> str:

        return f"Entry({repr(self.kind)}, 0o{self.mode:06o}, size={self.size}, object={repr(self.object)})"

    def __eq__(self, other: Any) -> bool:

        if not isinstance(other, Entry):
            raise TypeError(
                f"'==' not supported between '{type(self).__name__}' and '{type(other).__name__}'"
            )

        return (
            other.kind is self.kind
            and other.object == self.object
            and other.mode == self.mode
            and other.size == self.size
        )

    def __lt__(self, other: Any) -> bool:

        if not isinstance(other, Entry):
            raise TypeError(
                f"'<' not supported between '{type(self).__name__}' and '{type(other).__name__}'"
            )

        return other.kind is EntryKind.TREE

    def clone(self) -> "Entry":
        """Return a copy of this entry (does not clone)."""

        other = Entry(
            kind=self.kind,
            object=self.object,
            mode=self.mode,
            size=self.size,
        )
        for name, node in self.items():
            other[name] = node
        return other

    def update(self, other: "Manifest.Node") -> None:  # type: ignore

        self.kind = other.kind
        self.object = other.object
        self.mode = other.mode
        if self.kind is not EntryKind.TREE:
            self.size = other.size
            return

        for name, node in other.items():
            if node.kind == EntryKind.MASK:
                try:
                    del self[name]
                except KeyError:
                    continue

            if name not in self:
                self[name] = node
            else:
                self[name].update(node)
        self.size = len(self)
