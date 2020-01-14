from typing import Tuple, Dict, Optional, Iterator, TYPE_CHECKING, DefaultDict
from collections import OrderedDict
import os
import stat
import hashlib

from .. import runtime

from ._entry import EntryKind, Entry
from ._tree import Tree


if TYPE_CHECKING:
    EntryMap = OrderedDict[str, Entry]  # pylint: disable=unsubscriptable-object
else:
    EntryMap = OrderedDict


class Manifest:
    def __init__(self) -> None:

        self._root: Tree = Tree()
        self._trees: Dict[str, Tree] = {}

    @property
    def digest(self) -> str:

        return self._root.digest

    def get_path(self, path: str) -> Entry:

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

    def walk(self) -> Iterator[Tuple[str, Entry]]:
        def iter_tree(root: str, tree: Tree) -> Iterator[Tuple[str, Entry]]:

            for entry in tree:

                entry_path = os.path.join(root, entry.name)
                yield entry_path, entry

                if entry.kind is EntryKind.TREE:
                    sub_tree = self._trees[entry.object]
                    for item in iter_tree(entry_path, sub_tree):
                        yield item

        return iter_tree("/", self._root)

    def walk_abs(self, root: str) -> Iterator[Tuple[str, Entry]]:

        for relpath, entry in self.walk():
            yield os.path.join(root, relpath.lstrip("/")), entry

    def dump_dict(self) -> Dict:
        """Dump this manifest data into a dictionary of python basic types."""

        return {
            "digest": self.digest,
            "root": self._root.dump_dict(),
            "trees": list(t.dump_dict() for t in self._trees.values()),
        }

    @staticmethod
    def load_dict(data: Dict) -> "Manifest":
        """Load manifest data from the given dictionary dump."""

        if "root" not in data:
            # support for version < 0.12.15: less efficient manifest storage
            # had path-based data duplication. These versions also had a
            # bug where the stored trees were not actually valid, but were
            # not used at runtime because of the spare data. We can only
            # rebuild the manifest to ensure data integrity
            builder = ManifestBuilder("/")
            for path, entry_data in zip(data["paths"], data["entries"]):
                entry = Entry.load_dict(entry_data)
                builder.add_entry(path, entry)
            return builder.finalize()

        manifest = Manifest()
        manifest._root = Tree.load_dict(data["root"])
        for tree_data in data.get("trees", []):
            tree = Tree.load_dict(tree_data)
            assert tree.digest == tree_data["digest"], "Corrupt Manifest"
            manifest._trees[tree.digest] = tree
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
                manifest._trees["/"] = tree
                break

            parent = self._trees[os.path.dirname(tree_path)]
            tree_entry = self._tree_entries[tree_path]
            tree_entry = Entry(
                name=tree_entry.name,
                mode=tree_entry.mode,
                kind=tree_entry.kind,
                object=tree.digest,
            )
            parent.update(tree_entry)
            manifest._trees[tree.digest] = tree
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
                    Entry(kind=EntryKind.TREE, mode=0o775, object="", name=name),
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
