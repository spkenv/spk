from typing import NamedTuple, Tuple, List
import os
import enum
import uuid
import stat
import errno
import shutil
import hashlib

from ._runtime import Runtime


class Layer:

    _diff = "diff"
    dirs = (_diff,)

    def __init__(self, root: str):

        self._root = os.path.abspath(root)

    @property
    def ref(self):
        return os.path.basename(self._root)

    @property
    def diffdir(self):

        return os.path.join(self._root, self._diff)

    def compute_digest(self):

        return _compute_tree(self.diffdir).digest


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
    entries: Tuple[Entry]


def _compute_tree(dirname) -> Tree:
    # TODO: CLEAN ME

    names = os.listdir(dirname)
    entries = []
    for name in sorted(names):
        abspath = os.path.join(dirname, name)
        stat_result = os.lstat(abspath)
        kind = EntryKind.BLOB
        if stat.S_ISLNK(stat_result.st_mode):
            digest = hashlib.sha256(os.readlink(abspath)).hexdigest
        elif stat.S_ISDIR(stat_result.st_mode):
            kind = EntryKind.TREE
            digest = _compute_tree(abspath).digest
        elif not stat.S_ISREG(stat_result.st_mode):
            raise ValueError("unsupported file mode" + str(stat_result.st_mode))
        else:
            with open(abspath, "rb") as f:
                hasher = hashlib.sha256()
                for byte_block in iter(lambda: f.read(4096), b""):
                    hasher.update(byte_block)
                digest = hasher.hexdigest()

        entries.append(
            Entry(kind=kind, name=name, mode=stat_result.st_mode, digest=digest)
        )

    hasher = hashlib.sha256()
    for entry in entries:
        hasher.update(entry.digest.encode("ascii"))
    return Tree(digest=hasher.hexdigest(), entries=tuple(entries))


def _ensure_layer(path: str):

    os.makedirs(path, exist_ok=True)
    for subdir in Layer.dirs:
        os.makedirs(os.path.join(path, subdir), exist_ok=True)
    return Layer(path)


class LayerStorage:
    def __init__(self, root: str) -> None:

        self._root = os.path.abspath(root)

    def read_layer(self, ref: str) -> Layer:

        layer_path = os.path.join(self._root, ref)
        if not os.path.exists(layer_path):
            raise ValueError(f"Unknown layer: {ref}")
        return Layer(layer_path)

    def ensure_layer(self, ref: str) -> Layer:

        layer_dir = os.path.join(self._root, ref)
        return _ensure_layer(layer_dir)

    def remove_layer(self, ref: str) -> None:

        dirname = os.path.join(self._root, ref)
        try:
            shutil.rmtree(dirname)
        except OSError as e:
            if e.errno == errno.ENOENT:
                return
            raise

    def list_layers(self) -> List[Layer]:

        try:
            dirs = os.listdir(self._root)
        except OSError as e:
            if e.errno == errno.ENOENT:
                dirs = []
            else:
                raise

        return [Layer(os.path.join(self._root, d)) for d in dirs]

    def commit_runtime(self, runtime: Runtime) -> Layer:

        tmp_layer = self.ensure_layer(uuid.uuid1().hex)
        # FIXME: does this need to be unmounted from everywhere?
        # FIXME: does the runtime need to have a new parent and remount?
        os.rename(runtime.upperdir, tmp_layer.diffdir)
        os.mkdir(runtime.upperdir)
        digest = tmp_layer.compute_digest()
        new_root = os.path.join(self._root, digest)
        try:
            os.rename(tmp_layer._root, new_root)
        except OSError as e:
            if e.errno in (errno.EEXIST, errno.ENOTEMPTY):
                self.remove_layer(tmp_layer.ref)
            else:
                raise
        return self.read_layer(digest)
