from typing import Tuple, Dict, Optional, Iterable, TYPE_CHECKING, DefaultDict, BinaryIO
from collections import OrderedDict
import os
import stat
import hashlib

from .. import runtime, graph, encoding

from ._entry import EntryKind, Entry
from ._tree import Tree


if TYPE_CHECKING:
    EntryMap = OrderedDict[str, Entry]  # pylint: disable=unsubscriptable-object
else:
    EntryMap = OrderedDict


class Manifest(graph.Object):
    def __init__(self) -> None:

        self._root: Tree = Tree()
        self._trees: Dict[encoding.Digest, Tree] = {}

    def digest(self) -> encoding.Digest:

        return self._root.digest()

    def child_objects(self) -> Tuple[encoding.Digest, ...]:
        """Return the digests of objects that this manifest refers to."""

        children = []
        for _, entry in self.walk():
            if entry.kind is EntryKind.BLOB:
                children.append(entry.object)
        return tuple(children)

    def is_empty(self) -> bool:
        """Return true if this manifest has no contents."""

        return len(self._root) == 0

    def get_path(self, path: str) -> Entry:
        """Get an entry in this manifest given it's filepath.

        Raises:
            FileNotFoundError: if the entry does not exist
        """

        path = os.path.normpath(path).lstrip("/")
        steps = path.split("/")
        entry: Optional[Entry] = None
        tree: Optional[Tree] = self._root
        while steps:
            if tree is None:
                break
            step = steps.pop(0)
            entry = tree.get(step)
            if entry is None:
                break
            elif entry.kind is EntryKind.TREE:
                tree = self._trees[entry.object]
            elif entry.kind is EntryKind.BLOB:
                break

        if len(steps) or entry is None:
            raise FileNotFoundError(path)
        return entry

    def walk(self) -> Iterable[Tuple[str, Entry]]:
        """Walk the contents of this manifest depth-first."""

        def iter_tree(root: str, tree: Tree) -> Iterable[Tuple[str, Entry]]:

            for entry in tree:

                entry_path = os.path.join(root, entry.name)
                yield entry_path, entry

                if entry.kind is EntryKind.TREE:
                    sub_tree = self._trees[entry.object]
                    for item in iter_tree(entry_path, sub_tree):
                        yield item

        return iter_tree("/", self._root)

    def walk_abs(self, root: str) -> Iterable[Tuple[str, Entry]]:
        """Same as walk(), but joins all entry paths to the given root."""

        for relpath, entry in self.walk():
            yield os.path.join(root, relpath.lstrip("/")), entry

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


class ManifestBuilder:
    def __init__(self, root: str):

        self.root = os.path.abspath(root)
        self._tree_entries: Dict[str, Entry] = {}
        self._trees: Dict[str, Tree] = DefaultDict(Tree)
        self._makedirs("/")

    def finalize(self) -> Manifest:

        manifest = Manifest()
        # walk backwards to ensure that the finalization
        # of trees properly propagates up the structure
        for tree_path in reversed(sorted(self._trees)):
            tree = self._trees[tree_path]
            if tree_path == "/":
                manifest._root = tree
                manifest._trees[tree.digest()] = tree
                break

            parent = self._trees[os.path.dirname(tree_path)]
            tree_entry = self._tree_entries[tree_path]
            tree_entry = Entry(
                name=tree_entry.name,
                mode=tree_entry.mode,
                kind=tree_entry.kind,
                object=tree.digest(),
            )
            parent.update(tree_entry)
            manifest._trees[tree.digest()] = tree
        else:
            raise RuntimeError("Logic Error: root tree was never visited")

        return manifest

    def add_entry(self, path: str, entry: Entry) -> None:

        path = self._internal_path(path)
        if entry.kind is EntryKind.TREE:
            self._tree_entries[path] = entry
            assert self._trees[path] is not None, "Default dict failed to create tree"
        if path != "/":
            dirname = os.path.dirname(path)
            self._makedirs(dirname)
            self._trees[dirname].add(entry)

    def update_entry(self, path: str, entry: Entry) -> None:

        path = self._internal_path(path)
        if entry.kind is EntryKind.TREE:
            # only the mode bits are relevant in a dir update
            if path not in self._tree_entries:
                raise FileNotFoundError(path)
            self._tree_entries[path] = entry
        else:
            self.remove_entry(path)
            self.add_entry(path, entry)

    def remove_entry(self, path: str) -> None:

        path = self._internal_path(path)

        if path is "/":
            self._trees.clear()
            self._tree_entries.clear()

        dirname, basename = os.path.split(path)
        self._trees[dirname].remove(basename)
        for dirpath in list(self._tree_entries.keys()):
            if dirpath == path or dirpath.startswith(path + "/"):
                del self._tree_entries[dirpath]
                del self._trees[dirpath]

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
        if path not in self._trees:
            try:
                self.add_entry(
                    abspath,
                    Entry(
                        kind=EntryKind.TREE,
                        mode=0o775,
                        object=encoding.NULL_DIGEST,
                        name=name,
                    ),
                )
            except FileExistsError:
                pass


def compute_manifest(path: str) -> Manifest:

    manifest = ManifestBuilder(path)
    compute_entry(path, append_to=manifest)
    return manifest.finalize()


def compute_tree(dirname: str, append_to: ManifestBuilder = None) -> Tree:

    dirname = os.path.abspath(dirname)
    manifest = append_to or ManifestBuilder(dirname)
    names = sorted(os.listdir(dirname))
    paths = [os.path.join(dirname, n) for n in names]
    entries = []
    for path in paths:
        entry = compute_entry(path, append_to=manifest)
        entries.append(entry)

    return Tree(entries=tuple(entries))


def compute_entry(path: str, append_to: ManifestBuilder = None) -> Entry:

    path = os.path.abspath(path)
    manifest = append_to or ManifestBuilder(os.path.dirname(path))
    stat_result = os.lstat(path)

    kind = EntryKind.BLOB
    if stat.S_ISLNK(stat_result.st_mode):
        digest = encoding.Hasher(os.readlink(path).encode("utf-8")).digest()
    elif stat.S_ISDIR(stat_result.st_mode):
        kind = EntryKind.TREE
        digest = compute_tree(path, append_to=manifest).digest()
    elif runtime.is_removed_entry(stat_result):
        kind = EntryKind.MASK
        digest = encoding.Hasher().digest()  # emtpy/default digest
    elif not stat.S_ISREG(stat_result.st_mode):
        raise ValueError("unsupported special file: " + path)
    else:
        with open(path, "rb") as f:
            hasher = encoding.Hasher()
            for byte_block in iter(lambda: f.read(4096), b""):
                hasher.update(byte_block)
            digest = hasher.digest()

    entry = Entry(
        kind=kind, name=os.path.basename(path), mode=stat_result.st_mode, object=digest
    )
    try:
        manifest.add_entry(path, entry)
    except FileExistsError:
        # trees are automatically created in a makedirs fasion, so
        # it's entirely expected to hit entries that already exist
        manifest.update_entry(path, entry)
    return entry


def layer_manifests(*manifests: Manifest) -> Manifest:

    result = ManifestBuilder("/")
    for manifest in manifests:
        for path, entry in manifest.walk():

            if entry.kind == EntryKind.MASK:
                try:
                    result.remove_entry(path)  # manages recursive removal
                except FileNotFoundError:
                    # sometimes the parent may not have the file due to
                    # the specifics of how the stack was created. At the end
                    # of the day nonexistance is all that we care about
                    pass

            try:
                result.add_entry(path, entry)
            except FileExistsError:
                result.update_entry(path, entry)

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
