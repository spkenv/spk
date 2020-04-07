from typing import Tuple, Dict, Optional, Iterable, TYPE_CHECKING, DefaultDict, BinaryIO
from collections import OrderedDict
import os
import stat
import hashlib
import posixpath

from .. import runtime, graph, encoding

from ._entry import EntryKind, Entry


if TYPE_CHECKING:
    NodeMap = OrderedDict[str, Entry]  # pylint: disable=unsubscriptable-object
else:
    NodeMap = OrderedDict


class Manifest:

    __fields__ = ("root",)

    def __init__(self) -> None:

        self.root = Entry()

    def is_empty(self) -> bool:
        """Return true if this manifest has no contents."""

        return len(self.root) == 0

    def get_path(self, path: str) -> Entry:
        """Get an entry in this manifest given it's filepath.

        Raises:
            NotADirectoryError: if an element in the path is not a directory
            FileNotFoundError: if the entry does not exist
        """

        path = posixpath.normpath(path).strip("/.")
        entry = self.root
        if not path:
            return entry
        steps = path.split("/")
        i = 0
        while i < len(steps):
            if entry.kind is not EntryKind.TREE:
                raise NotADirectoryError("/".join(steps[:i]))
            step = steps[i]
            if step not in entry:
                raise FileNotFoundError("/".join(steps[:i]))
            entry = entry[step]
            i += 1

        return entry

    def list_dir(self, path: str) -> Tuple[str, ...]:
        """List the contents of a directory in this manifest.

        Raises:
            FileNotFoundError: if the directory does not exist
            NotADirectoryError: if the entry at the given path is not a tree
        """

        entry = self.get_path(path)
        if entry.kind is not EntryKind.TREE:
            raise NotADirectoryError(path)
        return tuple(entry.keys())

    def walk(self) -> Iterable[Tuple[str, Entry]]:
        """Walk the contents of this manifest depth-first."""

        def iter_node(root: str, entry: Entry) -> Iterable[Tuple[str, Entry]]:

            for name, entry in entry.items():
                full_path = os.path.join(root, name)
                yield full_path, entry
                if entry.kind is EntryKind.TREE:
                    for i in iter_node(full_path, entry):
                        yield i

        return iter_node("/", self.root)

    def walk_abs(self, root: str) -> Iterable[Tuple[str, Entry]]:
        """Same as walk(), but joins all entry paths to the given root."""

        for relpath, entry in self.walk():
            yield os.path.join(root, relpath.lstrip("/")), entry

    def mkdir(self, path: str) -> Entry:

        path = posixpath.normpath(path).strip("/.")
        if not path:
            raise FileExistsError(path)
        entry = self.root
        *dirname, name = path.split("/")
        for step in dirname:
            if step not in entry:
                raise FileNotFoundError(step)
            entry = entry[step]
            if entry.kind is not EntryKind.TREE:
                raise NotADirectoryError(step)
        if name in entry:
            raise FileExistsError(name)
        new_node = Entry()
        entry[name] = new_node
        return new_node

    def mkdirs(self, path: str) -> Entry:
        """Ensure that all levels of the given directory name exist.

        Entries that do not exist are created with a resonable default
        file mode, but can and should be replaces by a new entry in the
        case where this is not desired.
        """

        path = posixpath.normpath(path).strip("/.")
        if not path:
            return self.root
        entry = self.root
        for step in path.split("/"):
            if step not in entry:
                entry[step] = Entry()
            entry = entry[step]
            if entry.kind is not EntryKind.TREE:
                raise NotADirectoryError(step)
        return entry

    def mkfile(self, path: str) -> Entry:

        path = posixpath.normpath(path).strip("/")
        entry = self.root
        *dirname, name = path.split("/")
        for step in dirname:
            if step not in entry:
                raise FileNotFoundError(step)
            entry = entry[step]
            if entry.kind is not EntryKind.TREE:
                raise NotADirectoryError(step)
        if name in entry:
            raise FileExistsError(name)
        new_node = Entry()
        new_node.kind = EntryKind.BLOB
        entry[name] = new_node
        return new_node

    def update(self, other: "Manifest") -> None:

        self.root.update(other.root)


def compute_manifest(path: str) -> Manifest:

    manifest = Manifest()
    _compute_tree_node(path, manifest.root)
    return manifest


def _compute_tree_node(dirname: str, tree_node: Entry) -> None:

    for name in os.listdir(dirname):
        path = posixpath.join(dirname, name)
        entry = Entry()
        tree_node[name] = entry
        _compute_node(path, entry)


def _compute_node(path: str, entry: Entry) -> None:

    stat_result = os.lstat(path)

    entry.mode = stat_result.st_mode
    entry.size = stat_result.st_size

    if stat.S_ISLNK(stat_result.st_mode):
        entry.kind = EntryKind.BLOB
        entry.object = encoding.Hasher(os.readlink(path).encode("utf-8")).digest()
    elif stat.S_ISDIR(stat_result.st_mode):
        entry.kind = EntryKind.TREE
        _compute_tree_node(path, entry)
    elif runtime.is_removed_entry(stat_result):
        entry.kind = EntryKind.MASK
        entry.object = encoding.NULL_DIGEST
    elif not stat.S_ISREG(stat_result.st_mode):
        raise ValueError("unsupported special file: " + path)
    else:
        entry.kind = EntryKind.BLOB
        with open(path, "rb") as f:
            hasher = encoding.Hasher()
            for byte_block in iter(lambda: f.read(4096), b""):
                hasher.update(byte_block)
        entry.object = hasher.digest()


def sort_entries(entries: NodeMap) -> NodeMap:
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
            entry = entries.get(path)
            split_entries.append(entry)

        return tuple(split_entries)

    items = entries.items()
    return NodeMap(sorted(items, key=key))
