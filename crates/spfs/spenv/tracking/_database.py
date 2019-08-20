from typing import NamedTuple, Tuple, Dict, Union, Optional
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


class Tree(NamedTuple):

    digest: str
    entries: Tuple[Entry, ...]


class Database:
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


def compute_db(path: str) -> Database:

    db = Database()
    obj = compute_entry(path, append_to=db)
    return db


def compute_tree(dirname: str, append_to: Database = None) -> Tree:

    db = append_to or Database()
    names = sorted(os.listdir(dirname))
    paths = [os.path.join(dirname, n) for n in names]
    entries = []
    for path in paths:

        entry = compute_entry(path, append_to=db)
        entries.append(entry)

    hasher = hashlib.sha256()
    for entry in entries:
        hasher.update(entry.digest.encode("ascii"))

    tree = Tree(digest=hasher.hexdigest(), entries=tuple(entries))
    db._trees[tree.digest] = tree
    return tree


def compute_entry(path: str, append_to: Database = None) -> Entry:

    db = append_to or Database()
    stat_result = os.lstat(path)

    kind = EntryKind.BLOB
    if stat.S_ISLNK(stat_result.st_mode):
        digest = hashlib.sha256(os.readlink(path).encode("utf-8")).hexdigest()
    elif stat.S_ISDIR(stat_result.st_mode):
        kind = EntryKind.TREE
        digest = compute_tree(path, append_to=db).digest
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
    db._entries[entry.digest] = entry
    db._paths[path] = entry

    return entry
