from typing import Optional, List
import os
import uuid
import errno
import shutil
import hashlib


class Runtime:

    _upper = "upper"
    _work = "work"
    _lower = "lower"
    _merged = "merged"
    dirs = (_upper, _work, _lower, _merged)

    def __init__(self, root: str):

        self._root = os.path.abspath(root)

    @property
    def ref(self):
        return os.path.basename(self._root)

    @property
    def lowerdir(self):
        return os.path.join(self._root, self._lower)

    @property
    def upperdir(self):
        return os.path.join(self._root, self._upper)

    @property
    def workdir(self):
        return os.path.join(self._root, self._work)

    def _set_parent_ref(self, parent: Optional[str]) -> None:

        parent_file = os.path.join(self._root, "parent")
        if parent is None:
            try:
                return os.remove(parent_file)
            except OSError as e:
                if e.errno == errno.ENOENT:
                    return
                raise

        with open(parent_file, "bw+") as f:
            f.write(parent.encode("ascii"))

    def get_parent_ref(self) -> Optional[str]:

        parent_file = os.path.join(self._root, "parent")
        parent: Optional[str] = None
        try:
            with open(parent_file, encoding="ascii") as f:
                return f.read()
        except OSError as e:
            if e.errno == errno.ENOENT:
                pass
            else:
                raise
        return None


def _ensure_runtime(path: str):

    os.makedirs(path, exist_ok=True)
    for subdir in Runtime.dirs:
        os.makedirs(os.path.join(path, subdir), exist_ok=True)
    return Runtime(path)


class RuntimeStorage:
    def __init__(self, root: str) -> None:

        self._root = os.path.abspath(root)

    def read_runtime(self, ref: str) -> Runtime:

        runtime_path = os.path.join(self._root, ref)
        if not os.path.exists(runtime_path):
            raise ValueError(f"Unknown runtime: {ref}")
        return Runtime(runtime_path)

    def remove_runtime(self, ref: str) -> None:

        runtime_path = os.path.join(self._root, ref)
        shutil.rmtree(runtime_path)
        # FIXME: does this error when not exist?

    def list_runtimes(self) -> List[Runtime]:

        try:
            dirs = os.listdir(self._root)
        except OSError as e:
            if e.errno == errno.ENOENT:
                dirs = []
            else:
                raise

        return [Runtime(os.path.join(self._root, d)) for d in dirs]

    def create_runtime(self, parent: str = None) -> Runtime:

        ref = hashlib.sha256(uuid.uuid1().bytes).hexdigest()
        runtime_dir = os.path.join(self._root, ref)
        runtime = _ensure_runtime(runtime_dir)
        if parent is not None:
            runtime._set_parent_ref(parent)
        return runtime
