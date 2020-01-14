from typing import NamedTuple, Any, Dict
import enum
import hashlib


class EntryKind(enum.Enum):

    TREE = "tree"  # directory / node
    BLOB = "file"  # file / leaf
    MASK = "mask"  # removed entry / node or leaf


class Entry(NamedTuple):

    object: str
    kind: EntryKind
    mode: int
    name: str

    @property
    def digest(self) -> str:
        hasher = hashlib.sha256()
        hasher.update(f"{self.mode:06o}".encode("ascii"))
        hasher.update(self.kind.value.encode("utf-8"))
        hasher.update(self.name.encode("utf-8"))
        hasher.update(self.object.encode("ascii"))
        return hasher.hexdigest()

    def __str__(self) -> str:

        return f"{self.mode:06o} {self.kind.value} {self.name} {self.object}"

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

    def dump_dict(self) -> Dict:
        """Dump this entry data into a dictionary of python basic types."""

        return {
            "object": self.object,
            "kind": self.kind.value,
            "mode": self.mode,
            "name": self.name,
        }

    @staticmethod
    def load_dict(data: Dict) -> "Entry":
        """Load entry data from the given dictionary dump."""

        if "object" not in data:
            # support for version < 0.9
            # - the entry type used to store 'digest' as
            #   hash of whatever it pointed at, not it's
            #   own unique hash - but this was confusing
            #   in the API and had already been reponsible
            #   for an internal bug... :/
            data["object"] = data["digest"]

        return Entry(
            object=data["object"],
            kind=EntryKind(data["kind"]),
            mode=data["mode"],
            name=data["name"],
        )
