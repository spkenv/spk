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

from .. import runtime


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
            # support for spenv version < 0.9
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


class Tree(NamedTuple):

    entries: Tuple[Entry, ...]

    @property
    def digest(self) -> str:
        hasher = hashlib.sha256()
        for entry in self.entries:
            hasher.update(str(entry).encode("ascii"))
        return hasher.hexdigest()

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

        return Tree(entries=tuple(Entry.load_dict(e) for e in data.get("entries", [])))


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
        return tree.object

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
        self._by_path: EntryMap = OrderedDict()
        self._by_dir: Dict[str, EntryMap] = {}

    def finalize(self) -> Manifest:

        self.sort()
        sorted_trees = []
        # walk backwards to ensure that the finalization
        # of trees properly propagates up the structure
        for dirname, entry in reversed(self._by_path.items()):
            if entry.kind is not EntryKind.TREE:
                continue
            entries = self._by_dir.get(dirname) or OrderedDict()
            entries = sort_entries(entries)
            tree_entry = self._by_path[dirname]
            tree = Tree(entries=tuple(entries.values()))
            sorted_trees.append(tree)
            self._by_path[dirname] = Entry(
                name=tree_entry.name,
                mode=tree_entry.mode,
                kind=tree_entry.kind,
                object=tree.digest,
            )

        self.sort()
        return Manifest(
            paths=tuple(self._by_path.keys()),
            entries=tuple(self._by_path.values()),
            trees=tuple(sorted_trees),
        )

    def add_entry(self, path: str, entry: Entry) -> None:

        path = self._internal_path(path)
        self._by_path[path] = entry
        dirname, basename = os.path.split(path)
        dirmap = self._by_dir.get(dirname, EntryMap())
        if path != "/":
            self._makedirs(dirname)
            # the entrymap is expected to hold absolute paths, same as the manifest
            dirmap["/" + basename] = entry
        self._by_dir[dirname] = dirmap

    def remove_entry(self, path: str) -> None:

        path = self._internal_path(path)

        for name in list(self._by_path.keys()):
            if name.startswith(path):
                del self._by_path[name]
        for name in list(self._by_dir.keys()):
            if name.startswith(path):
                del self._by_dir[name]

    def _internal_path(self, path: str) -> str:

        assert path.startswith(
            self.root
        ), f"Must be a path under: {self.root}, got: {path}"

        return os.path.join("/", path[len(self.root) :])

    def _makedirs(self, path: str) -> None:
        """Ensure that all levels of the given directory name exist.

        Entries that do not exist are created with a resonable default
        file mode, but can and should be replaces by a new entry in the
        case where this is not desired.
        """

        abspath = os.path.join(self.root, path.lstrip("/"))
        name = os.path.basename(abspath)
        if path not in self._by_path:
            self.add_entry(
                abspath, Entry(kind=EntryKind.TREE, mode=0o775, object="", name=name)
            )

    def sort(self) -> None:
        self._by_path = sort_entries(self._by_path)


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

    return Tree(entries=tuple(entries))


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
    elif runtime.is_removed_entry(stat_result):
        kind = EntryKind.MASK
        digest = hashlib.sha256().hexdigest()  # emtpy/default digest
    elif not stat.S_ISREG(stat_result.st_mode):
        raise ValueError("unsupported special file: " + path)
    else:
        with open(path, "rb") as f:
            hasher = hashlib.sha256()
            for byte_block in iter(lambda: f.read(4096), b""):
                hasher.update(byte_block)
            digest = hasher.hexdigest()

    entry = Entry(
        kind=kind, name=os.path.basename(path), mode=stat_result.st_mode, object=digest
    )
    manifest.add_entry(path, entry)
    return entry


def layer_manifests(*manifests: Manifest) -> Manifest:

    result = MutableManifest("/")
    for manifest in manifests:
        for path, entry in manifest.walk():

            if entry.kind == EntryKind.MASK:
                result.remove_entry(path)  # manages recursive removal

            result.add_entry(path, entry)

    return result.finalize()


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
            entry = entries.get(path)
            split_entries.append(entry)

        return tuple(split_entries)

    items = entries.items()
    return EntryMap(sorted(items, key=key))
