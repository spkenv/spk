from typing import Iterable, Dict, Any, BinaryIO, Union, Tuple, Iterator, TypeVar
from typing_extensions import Protocol, runtime_checkable
import abc

from sortedcontainers import SortedDict

from .. import tracking, encoding, graph

_T = TypeVar("_T")


class Entry(encoding.Encodable):
    def __init__(self, name: str, entry: tracking.Entry) -> None:

        self.object = entry.object
        self.kind = entry.kind
        self.mode = entry.mode
        self.size = entry.size
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

        return other.kind is tracking.EntryKind.TREE

    def __gt__(self, other: Any) -> bool:

        if not isinstance(other, Entry):
            raise TypeError(
                f"'>' not supported between '{type(self).__name__}' and '{type(other).__name__}'"
            )

        if self.kind is other.kind:
            return self.name > other.name

        return self.kind is tracking.EntryKind.TREE

    def encode(self, writer: BinaryIO) -> None:

        encoding.write_digest(writer, self.object)
        encoding.write_string(writer, self.kind.value)
        encoding.write_int(writer, self.mode)
        encoding.write_int(writer, self.size)
        encoding.write_string(writer, self.name)

    @classmethod
    def decode(cls, reader: BinaryIO) -> "Entry":

        return Entry(
            entry=tracking.Entry(
                object=encoding.read_digest(reader),
                kind=tracking.EntryKind(encoding.read_string(reader)),
                mode=encoding.read_int(reader),
                size=encoding.read_int(reader),
            ),
            name=encoding.read_string(reader),
        )


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

    def get(self, key: Union[int, str], default: _T = None) -> Union[Entry, _T, None]:

        try:
            return self[key]
        except KeyError:
            return default

    def __iter__(self) -> Iterator[Entry]:

        for _, entry in self._entries.items():
            yield entry

    def __len__(self) -> int:
        return len(self._entries)

    def list(self) -> Tuple[Entry, ...]:
        """Return the entries in this tree."""
        return tuple(self._entries.values())

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
            entry = Entry.decode(reader)
            tree._entries[entry.name] = entry
        return tree


class Manifest(graph.Object):

    __fields__ = ("_root", "_trees")

    def __init__(self, source: tracking.Manifest = None) -> None:

        self._root: Tree = Tree()
        self._trees: Dict[encoding.Digest, Tree] = {}

        if source is None:
            return

        def _build_tree(source_node: tracking.Entry) -> Tree:

            dest_tree = Tree()
            for name, entry in source_node.items():

                entry = entry.clone()
                if entry.kind is tracking.EntryKind.TREE:
                    tree = _build_tree(entry)
                    self._trees[tree.digest()] = tree
                    entry.object = tree.digest()
                    entry.size = len(tree)

                dest_tree.add(Entry(name, entry))
            return dest_tree

        self._root = _build_tree(source.root)

    @property
    def root(self) -> Tree:
        """Return the root tree object of this manifest."""
        return self._root

    def child_objects(self) -> Tuple[encoding.Digest, ...]:
        """Return the digests of objects that this manifest refers to."""

        children = set()
        for tree in (self._root, *self._trees.values()):
            for entry in tree.list():
                if entry.kind is tracking.EntryKind.BLOB:
                    children.add(entry.object)
        return tuple(children)

    def iter_entries(self) -> Iterable[Entry]:
        """Iterate all of the entries in this manifest"""

        for entry in self._root.list():
            yield entry
        for tree in self._trees.values():
            for entry in tree.list():
                yield entry

    def unlock(self) -> tracking.Manifest:
        manifest = tracking.Manifest()

        def iter_tree(tree: Tree, parent: tracking.Entry) -> None:

            for entry in tree:

                new_entry = tracking.Entry()
                new_entry.kind = entry.kind
                new_entry.mode = entry.mode
                if entry.kind is not tracking.EntryKind.TREE:
                    new_entry.object = entry.object
                new_entry.size = entry.size
                parent[entry.name] = new_entry
                if entry.kind is tracking.EntryKind.TREE:
                    iter_tree(self._trees[entry.object], new_entry)

        iter_tree(self.root, manifest.root)
        return manifest

    def encode(self, writer: BinaryIO) -> None:

        self._root.encode(writer)
        encoding.write_int(writer, len(self._trees))
        for tree in self._trees.values():
            tree.encode(writer)

    @classmethod
    def decode(cls, reader: BinaryIO) -> "Manifest":

        manifest = Manifest()
        manifest._root = Tree.decode(reader)
        num_trees = encoding.read_int(reader)
        for _ in range(num_trees):
            tree = Tree.decode(reader)
            manifest._trees[tree.digest()] = tree
        return manifest


class ManifestStorage:
    def __init__(self, db: graph.Database) -> None:

        self._db = db

    def iter_manifests(self) -> Iterable[Manifest]:
        """Iterate the objects in this storage which are manifests."""

        for obj in self._db.iter_objects():
            if isinstance(obj, Manifest):
                yield obj

    def has_manifest(self, digest: encoding.Digest) -> bool:
        """Return true if the identified manifest exists in this storage."""

        try:
            self.read_manifest(digest)
        except graph.UnknownObjectError:
            return False
        except AssertionError:
            return False
        return True

    def read_manifest(self, digest: encoding.Digest) -> Manifest:
        """Return the manifest identified by the given digest.

        Raises:
            AssertionError: if the identified object is not a manifest
        """

        obj = self._db.read_object(digest)
        assert isinstance(
            obj, Manifest
        ), f"Loaded object is not a manifest, got: {type(obj).__name__}"
        return obj


class ManifestViewer(metaclass=abc.ABCMeta):
    @abc.abstractmethod
    def render_manifest(self, manifest: Manifest) -> str:
        """Create a rendered view of the given manifest on the local disk.

        Returns:
            str: the local path to the root of the rendered manifest
        """
        ...

    @abc.abstractmethod
    def remove_rendered_manifest(self, digest: encoding.Digest) -> None:
        """Cleanup a previously rendered manifest from the local disk."""
        ...
