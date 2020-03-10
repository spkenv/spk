from typing import TypeVar, List, Dict, Iterable, Union, Any, Iterator, BinaryIO

from sortedcontainers import SortedDict

from .. import encoding
from ._entry import Entry

T = TypeVar("T")


class Tree(encoding.Encodable):
    """Tree is an ordered collection of entries.

    Only one entry of a given name is allowed at a time.
    """

    def __init__(self, entries: Iterable[Entry] = []) -> None:

        self._entries: Dict[str, Entry] = SortedDict()
        for entry in entries:
            self.add(entry)

    def __repr__(self) -> str:

        return f"<{self.__class__.__name__} '{self.digest().str()}'>"

    def __getitem__(self, key: Union[int, str]) -> Entry:

        if isinstance(key, int):
            return self._entries.values()[key]  # type: ignore
        return self._entries[key]

    def __eq__(self, other: Any) -> bool:

        if isinstance(other, Tree):
            return self.digest() == other.digest()
        return super(Tree, self).__eq__(other)

    def get(self, key: Union[int, str], default: T = None) -> Union[Entry, T, None]:

        try:
            return self[key]
        except KeyError:
            return default

    def __iter__(self) -> Iterator[Entry]:

        for _, entry in self._entries.items():
            yield entry

    def __len__(self) -> int:
        return len(self._entries)

    def add(self, entry: Entry) -> None:
        """Add an enry to this tree.

        Raises:
            ValueError: if an entry with the same name exists
        """
        if entry.name in self._entries:
            raise FileExistsError(entry.name)
        self._entries[entry.name] = entry

    def update(self, entry: Entry) -> None:
        self.remove(entry.name)
        self.add(entry)

    def remove(self, name: str) -> Entry:

        try:
            return self._entries.pop(name)
        except KeyError:
            raise FileNotFoundError(name)

    def encode(self, writer: BinaryIO) -> None:

        encoding.write_int(writer, len(self._entries))
        for entry in self._entries.values():
            entry.encode(writer)

    @classmethod
    def decode(cls, reader: BinaryIO) -> "Tree":

        tree = Tree()
        entry_count = encoding.read_int(reader)
        for _ in range(entry_count):
            tree.add(Entry.decode(reader))
        return tree
