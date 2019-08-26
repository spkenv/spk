from typing import Dict
import os
import errno
import shutil

import yaml

from ._manifest import Manifest, EntryKind


class ManifestWriter:
    def __init__(self, target_dir: str) -> None:

        self._dir = os.path.abspath(target_dir)

    def rewrite_db(self, manifest: Manifest, prefix: str = "") -> None:

        for name in os.listdir(self._dir):
            if name == ".git":
                continue
            abspath = os.path.join(self._dir, name)
            try:
                os.remove(abspath)
            except OSError as e:
                if e.errno == errno.EISDIR:
                    shutil.rmtree(abspath)
                elif e.errno == errno.ENOENT:
                    pass
                else:
                    raise

        self.write_db(manifest, prefix)

    def write_db(self, manifest: Manifest, prefix: str = "") -> None:

        serialized: Dict = {}
        for path, entry in manifest.walk():

            relpath = path[len(prefix) :] or "/"
            if entry.kind is EntryKind.TREE:
                serialized[relpath] = []
                continue

            relpath = os.path.dirname(relpath)
            serialized[relpath].append(entry.serialize())

            metapath = os.path.join(self._dir, "entries.yaml")
            with open(metapath, "w+", encoding="utf-8") as f:
                yaml.safe_dump(serialized, f, sort_keys=True)


class ManifestReader:
    def __init__(self, target_dir: str) -> None:

        self._dir = os.path.abspath(target_dir)

    def read(self) -> Manifest:

        manifest = Manifest()
        metapath = os.path.join(self._dir, "entries.yaml")
        with open(metapath, "w+", encoding="utf-8") as f:
            serialized = yaml.safe_load(f)

        raise NotImplementedError(serialized)
