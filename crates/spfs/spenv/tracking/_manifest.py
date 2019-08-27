from typing import Any, NamedTuple, Tuple, Dict, Union, Optional, Iterator, OrderedDict
import os
import enum
import stat
import hashlib
import operator
import collections


class EntryKind(enum.Enum):

    TREE = "tree"
    BLOB = "file"


class Entry(NamedTuple):

    digest: str
    kind: EntryKind
    mode: int
    name: str

    def serialize(self) -> str:

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


class Tree(NamedTuple):

    digest: str
    entries: Tuple[Entry, ...]


class Manifest:
    def __init__(self, root: str):

        self._root = os.path.abspath(root)
        self._paths: OrderedDict[str, Entry] = collections.OrderedDict()
        self._entries: Dict[str, Entry] = {}
        self._trees: Dict[str, Tree] = {}

    def get_path(self, path: str) -> Optional[Entry]:

        path = self._clean_path(path)
        return self._paths.get(path)

    def get_entry(self, digest: str) -> Optional[Entry]:

        return self._entries.get(digest)

    def get_tree(self, digest: str) -> Optional[Tree]:

        return self._trees.get(digest)

    def list_entries(self) -> Tuple[Entry, ...]:

        return tuple(self._entries.values())

    def list_trees(self) -> Tuple[Tree, ...]:

        return tuple(self._trees.values())

    def walk(self) -> Iterator[Tuple[str, Entry]]:

        return self._paths.items()

    def _clean_path(self, path: str) -> str:

        if os.path.isabs(path):
            path = os.path.relpath(path, self._root)
        path = os.path.normpath(path)
        if path[0] != ".":
            path = os.path.join(".", path)
        return path

    def _add_tree(self, tree: Tree) -> None:
        self._trees[tree.digest] = tree

    def _add_entry(self, path: str, entry: Entry) -> None:

        path = self._clean_path(path)
        self._entries[entry.digest] = entry
        self._paths[path] = entry

    def sort(self) -> None:
        def key(item: Tuple[str, Entry]) -> Tuple:

            entries = []
            parts = item[0].split(os.sep)
            path = ""
            for part in parts:
                path = os.path.join(path, part)
                entry = self.get_path(path)
                assert entry is not None, "Cannot sort, missing entry for: " + path
                entries.append(entry)

            return tuple(entries)

        items = self._paths.items()
        self._paths = OrderedDict(sorted(items, key=key))


def compute_manifest(path: str) -> Manifest:

    manifest = Manifest(path)
    compute_entry(path, append_to=manifest)
    manifest.sort()
    return manifest


def compute_tree(dirname: str, append_to: Manifest = None) -> Tree:

    dirname = os.path.abspath(dirname)
    manifest = append_to or Manifest(dirname)
    names = sorted(os.listdir(dirname))
    paths = [os.path.join(dirname, n) for n in names]
    entries = []
    for path in paths:

        if os.path.basename(path) == ".spenv":
            continue  # TODO: clean this up? at least constant
        entry = compute_entry(path, append_to=manifest)
        entries.append(entry)

    hasher = hashlib.sha256()
    for entry in entries:
        hasher.update(entry.digest.encode("ascii"))

    tree = Tree(digest=hasher.hexdigest(), entries=tuple(entries))
    manifest._add_tree(tree)
    return tree


def compute_entry(path: str, append_to: Manifest = None) -> Entry:

    path = os.path.abspath(path)
    manifest = append_to or Manifest(os.path.dirname(path))
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
    manifest._add_entry(path, entry)
    return entry
