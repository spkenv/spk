from typing import Dict
import os
import errno
import shutil

import yaml

from ._manifest import Manifest, EntryKind


class ManifestWriter:
    def __init__(self, target_dir: str) -> None:

        self._dir = os.path.abspath(target_dir)

    def rewrite(self, manifest: Manifest) -> None:

        try:
            shutil.rmtree(self._dir)
        except OSError as e:
            if e.errno != errno.ENOENT:
                raise

        os.makedirs(self._dir, exist_ok=True)
        self.write(manifest)

    def write(self, manifest: Manifest) -> None:

        serialized: Dict = {}
        for path, entry in manifest.walk():

            if entry.kind is EntryKind.TREE:
                serialized[path] = []
                continue

            path = os.path.dirname(path)
            serialized[path].append(entry.serialize())

            metapath = os.path.join(self._dir, "entries.yaml")
            with open(metapath, "w+", encoding="utf-8") as f:
                yaml.safe_dump(serialized, f, sort_keys=True)


class ManifestReader:
    def __init__(self, target_dir: str) -> None:

        self._dir = os.path.abspath(target_dir)

    def read(self) -> Manifest:

        manifest = Manifest(self._dir)
        metapath = os.path.join(self._dir, "entries.yaml")
        with open(metapath, "w+", encoding="utf-8") as f:
            serialized = yaml.safe_load(f)

        raise NotImplementedError(serialized)
