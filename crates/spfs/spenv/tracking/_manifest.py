from typing import (
    Any,
    NamedTuple,
    Tuple,
    Dict,
    Union,
    Optional,
    Iterator,
    List,
    TYPE_CHECKING,
)
from collections import OrderedDict
import os
import enum
import stat
import hashlib
import operator


class EntryKind(enum.Enum):

    TREE = "tree"
    BLOB = "file"


class Entry(NamedTuple):

    digest: str
    kind: EntryKind
    mode: int
    name: str

    def __str__(self) -> str:

        return f"{self.mode:03o} {self.kind.value} {self.name} {self.digest}"

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
            "digest": self.digest,
            "kind": self.kind.value,
            "mode": self.mode,
            "name": self.name,
        }

    @staticmethod
    def load_dict(data: Dict) -> "Entry":
        """Load entry data from the given dictionary dump."""

        return Entry(
            digest=data["digest"],
            kind=EntryKind(data["kind"]),
            mode=data["mode"],
            name=data["name"],
        )


class Tree(NamedTuple):

    digest: str
    entries: Tuple[Entry, ...]

    def dump_dict(self) -> Dict:
        """Dump this tree data into a dictionary of python basic types."""

        # TODO: entry data is getting duplicated here when dumping a manifest...
        return {
            "digest": self.digest,
            "entries": list(e.dump_dict() for e in self.entries),
        }

    @staticmethod
    def load_dict(data: Dict) -> "Tree":
        """Load tree data from the given dictionary dump."""

        return Tree(
            digest=data["digest"],
            entries=tuple(Entry.load_dict(e) for e in data.get("entries", [])),
        )


if TYPE_CHECKING:
    EntryMap = OrderedDict[str, Entry]
else:
    EntryMap = OrderedDict


class Manifest(NamedTuple):

    paths: Tuple[str, ...] = tuple()
    entries: Tuple[Entry, ...] = tuple()
    trees: Tuple[Tree, ...] = tuple()

    @property
    def digest(self) -> str:

        tree = self.get_path("/")
        assert tree is not None, "Manifest is incomplete or corrupted (no root entry)"
        return tree.digest

    def get_path(self, path: str) -> Optional[Entry]:

        path = os.path.join("/", path)
        for i in range(len(self.paths)):
            if self.paths[i] == path:
                return self.entries[i]
        else:
            return None

    def get_entry(self, digest: str) -> Optional[Entry]:

        for entry in self.entries:
            if entry.digest == digest:
                return entry
        else:
            return None

    def get_tree(self, digest: str) -> Optional[Tree]:

        for tree in self.trees:
            if tree.digest == digest:
                return tree
        else:
            return None

    def walk(self) -> Iterator[Tuple[str, Entry]]:

        return zip(self.paths, self.entries)

    def walk_abs(self, root: str) -> Iterator[Tuple[str, Entry]]:

        for relpath, entry in self.walk():
            yield os.path.join(root, relpath.lstrip("/")), entry

    def dump_dict(self) -> Dict:
        """Dump this manifest data into a dictionary of python basic types."""

        return {
            "paths": list(self.paths),
            "entries": list(e.dump_dict() for e in self.entries),
            "trees": list(t.dump_dict() for t in self.trees),
        }

    @staticmethod
    def load_dict(data: Dict) -> "Manifest":
        """Load manifest data from the given dictionary dump."""

        return Manifest(
            paths=tuple(data.get("paths", [])),
            entries=tuple(Entry.load_dict(e) for e in data.get("entries", [])),
            trees=tuple(Tree.load_dict(t) for t in data.get("trees", [])),
        )


class MutableManifest:
    def __init__(self, root: str):

        self.root = os.path.abspath(root)
        self._paths: EntryMap = OrderedDict()
        self._entries: Dict[str, Entry] = {}
        self._trees: Dict[str, Tree] = {}

    def finalize(self) -> Manifest:

        self.sort()
        sorted_trees = []
        for _, entry in self._paths.items():
            if entry.digest not in self._trees:
                continue
            sorted_trees.append(self._trees[entry.digest])
        return Manifest(
            paths=tuple(self._paths.keys()),
            entries=tuple(self._paths.values()),
            trees=tuple(sorted_trees),
        )

    def add_tree(self, tree: Tree) -> None:
        self._trees[tree.digest] = tree

    def add_entry(self, path: str, entry: Entry) -> None:

        assert path.startswith(
            self.root
        ), f"Must be a path under: {self.root}, got: {path}"
        path = os.path.join("/", path[len(self.root) :])
        self._entries[entry.digest] = entry
        self._paths[path] = entry

    def sort(self) -> None:
        self._paths = sort_entries(self._paths)


def compute_manifest(path: str) -> Manifest:

    manifest = MutableManifest(path)
    compute_entry(path, append_to=manifest)
    manifest.sort()
    return manifest.finalize()


def compute_tree(dirname: str, append_to: MutableManifest = None) -> Tree:

    dirname = os.path.abspath(dirname)
    manifest = append_to or MutableManifest(dirname)
    names = sorted(os.listdir(dirname))
    paths = [os.path.join(dirname, n) for n in names]
    entries = []
    for path in paths:
        entry = compute_entry(path, append_to=manifest)
        entries.append(entry)

    hasher = hashlib.sha256()
    for entry in entries:
        hasher.update(entry.digest.encode("ascii"))

    tree = Tree(digest=hasher.hexdigest(), entries=tuple(entries))
    manifest.add_tree(tree)
    return tree


def compute_entry(path: str, append_to: MutableManifest = None) -> Entry:

    path = os.path.abspath(path)
    manifest = append_to or MutableManifest(os.path.dirname(path))
    stat_result = os.lstat(path)

    kind = EntryKind.BLOB
    if stat.S_ISLNK(stat_result.st_mode):
        digest = hashlib.sha256(os.readlink(path).encode("utf-8")).hexdigest()
    elif stat.S_ISDIR(stat_result.st_mode):
        kind = EntryKind.TREE
        digest = compute_tree(path, append_to=manifest).digest
    elif not stat.S_ISREG(stat_result.st_mode):
        raise ValueError("unsupported file mode" + str(stat_result.st_mode))
    else:
        with open(path, "rb") as f:
            hasher = hashlib.sha256()
            for byte_block in iter(lambda: f.read(4096), b""):
                hasher.update(byte_block)
            digest = hasher.hexdigest()

    entry = Entry(
        kind=kind, name=os.path.basename(path), mode=stat_result.st_mode, digest=digest
    )
    manifest.add_entry(path, entry)
    return entry


def sort_entries(entries: EntryMap) -> EntryMap:
    """Sort a set of entries organized by file path.

    The given entry set must be complete, meaning that
    if an entry is specified at path '/dir/file.txt', then
    an entry must exist for '/dir' and '/'

    Raises:
        KeyError: if a required entry is missing from the map
    """

    def key(item: Tuple[str, Entry]) -> Tuple:

        split_entries = []
        parts = item[0].rstrip("/").split(os.sep)
        path = "/"
        for part in parts:
            path = os.path.join(path, part)
            entry = entries[path]
            split_entries.append(entry)

        return tuple(split_entries)

    items = entries.items()
    return EntryMap(sorted(items, key=key))
