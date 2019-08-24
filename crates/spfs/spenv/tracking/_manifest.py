from typing import NamedTuple, Tuple, Dict, Union, Optional, Iterator
import os
import enum
import stat
import hashlib
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


class Tree(NamedTuple):

    digest: str
    entries: Tuple[Entry, ...]


class Manifest:
    def __init__(self):

        self._paths: OrderedDict[str, Entry] = collections.OrderedDict()
        self._entries: Dict[str, Entry] = {}
        self._trees: Dict[str, Tree] = {}

    def get_path(self, path: str) -> Optional[Entry]:

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


def compute_manifest(path: str) -> Manifest:

    manifest = Manifest()
    obj = compute_entry(path, append_to=manifest)
    return manifest


def compute_tree(dirname: str, append_to: Manifest = None) -> Tree:

    manifest = append_to or Manifest()
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
    manifest._trees[tree.digest] = tree
    return tree


def compute_entry(path: str, append_to: Manifest = None) -> Entry:

    manifest = append_to or Manifest()
    stat_result = os.lstat(path)

    # entry placeholder goes in to ensure appropriate ordering
    # of the path dictionary in the database
    manifest._paths[path] = None  # type: ignore

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
    manifest._entries[entry.digest] = entry
    manifest._paths[path] = entry

    return entry
