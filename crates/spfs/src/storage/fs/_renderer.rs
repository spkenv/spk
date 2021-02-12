from typing import IO, List, TYPE_CHECKING
import os
import io
import json
import stat
import uuid
import shutil
import hashlib

import structlog

from ... import tracking, graph, encoding
from .. import Manifest, ManifestViewer, PayloadStorage
from ._database import FSDatabase, FSPayloadStorage

_CHUNK_SIZE = 1024

_LOGGER = structlog.get_logger(__name__)


class FSManifestViewer(ManifestViewer, FSDatabase):
    def __init__(self, root: str, payloads: PayloadStorage) -> None:

        self._storage = payloads
        ManifestViewer.__init__(self)
        FSDatabase.__init__(self, root)

    def render_manifest(self, manifest: Manifest) -> str:
        """Create a hard-linked rendering of the given file manifest.

        Raises:
            ValueErrors: if any of the blobs in the manifest are
                not available in this storage.

        Returns:
            str: the path to the root of the rendered manifest
        """

        rendered_dirpath = self._build_digest_path(manifest.digest())
        if _was_render_completed(rendered_dirpath):
            return rendered_dirpath

        self._ensure_base_dir(rendered_dirpath)
        try:
            os.mkdir(rendered_dirpath)
        except FileExistsError:
            pass

        walkable = manifest.unlock()
        for rendered_path, entry in walkable.walk_abs(rendered_dirpath):
            if entry.kind is tracking.EntryKind.TREE:
                os.makedirs(rendered_path, exist_ok=True)
            elif entry.kind is tracking.EntryKind.MASK:
                continue
            elif entry.kind is tracking.EntryKind.BLOB:
                self._render_blob(rendered_path, entry)
            else:
                raise NotImplementedError(f"Unsupported entry kind: {entry.kind}")

        for rendered_path, entry in reversed(list(walkable.walk_abs(rendered_dirpath))):
            if entry.kind is tracking.EntryKind.MASK:
                continue
            if stat.S_ISLNK(entry.mode):
                continue
            os.chmod(rendered_path, entry.mode)

        _mark_render_completed(rendered_dirpath)
        return rendered_dirpath

    def _render_blob(self, rendered_path: str, entry: tracking.Entry) -> None:

        if stat.S_ISLNK(entry.mode):

            try:
                with self._storage.open_payload(entry.object) as reader:
                    target = reader.read()
            except FileNotFoundError:
                raise graph.UnknownObjectError(entry.object)
            try:
                os.symlink(target, rendered_path)
            except FileExistsError:
                pass
            return

        source = self._storage
        if isinstance(source, FSPayloadStorage):
            committed_path = source._build_digest_path(entry.object)
            try:
                os.link(committed_path, rendered_path)
            except FileExistsError:
                return
            else:
                return

        try:
            with source.open_payload(entry.object) as reader:
                with open(rendered_path, "xb") as writer:
                    shutil.copyfileobj(reader, writer)
        except FileExistsError:
            return

    def remove_rendered_manifest(self, digest: encoding.Digest) -> None:
        """Remove the identified render from this storage."""

        rendered_dirpath = self._build_digest_path(digest)
        working_dirname = "work-" + uuid.uuid4().hex
        working_dirpath = os.path.join(self.root, working_dirname)
        try:
            os.rename(rendered_dirpath, working_dirpath)
        except FileNotFoundError:
            return

        _unmark_render_completed(rendered_dirpath)
        for root, dirs, files in os.walk(working_dirpath, topdown=False):

            for name in files:
                path = os.path.join(root, name)
                os.chmod(path, 0o0777)
                os.remove(path)
            for name in dirs:
                path = os.path.join(root, name)
                os.chmod(path, 0o0777)
                os.rmdir(path)

        os.rmdir(working_dirpath)


def _was_render_completed(render_path: str) -> bool:

    return os.path.exists(render_path + ".completed")


def _mark_render_completed(render_path: str) -> None:

    open(render_path + ".completed", "w+").close()


def _unmark_render_completed(render_path: str) -> None:

    try:
        os.remove(render_path + ".completed")
    except FileNotFoundError:
        pass


def _copy_manifest(manifest: Manifest, src_root: str, dst_root: str) -> None:
    """Copy manifest contents from one directory to another."""

    src_root = src_root.rstrip("/")
    dst_root = dst_root.rstrip("/")

    unlocked = manifest.unlock()

    def get_masked_entries(dirname: str, entry_names: List[str]) -> List[str]:

        ignored = []
        manifest_path = dirname[len(src_root) :] or "/"
        for name in entry_names:
            entry_path = os.path.join(manifest_path, name)
            entry = unlocked.get_path(entry_path)
            if entry.kind is tracking.EntryKind.MASK:
                ignored.append(name)
        return ignored

    shutil.copytree(
        src_root,
        dst_root,
        symlinks=True,
        ignore_dangling_symlinks=True,
        ignore=get_masked_entries,
    )
