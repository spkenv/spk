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
from .. import ManifestViewer, PayloadStorage
from ._database import FileDB, FSPayloadStorage

_CHUNK_SIZE = 1024

_LOGGER = structlog.get_logger(__name__)


# TODO: class BlobStorage(FileDB):
#     """Manages a local file system storage of arbitrary binary data.

#     Also provides harlinked renders of file manifests for use
#     in local runtimes.
#     """

#     def __init__(self, root: str) -> None:

#         super(BlobStorage, self).__init__(root)
#         # self.renders = ManifestStorage(os.path.join(self.__root, "renders"))

#         # this default is appropriate for shared repos, but can be locked further
#         # in cases where the current user will own the files, and other don't need
#         # to modify the storage (0x444)
#         # this is because on filesystems with protected hardlinks enabled I either
#         # need to own the file or have read+write+exec access to it
#         self.blob_permissions = 0o777

#     def has_object(self, digest: encoding.Digest) -> bool:
#         """Return true if the identified blob exists in this storage."""
#         try:
#             self.open_blob(digest).close()
#         except graph.UnknownObjectError:
#             return False
#         else:
#             return True

#     def open_blob(self, digest: encoding.Digest) -> IO[bytes]:
#         """Return a handle to the blob identified by the given digest.

#         Raises:
#             ValueError: if the blob does not exist in this storage
#         """
#         try:
#             filepath = self._build_digest_path(digest)
#             return open(filepath, "rb")
#         except FileNotFoundError:
#             raise graph.UnknownObjectError(digest)

#     def write_blob(self, data: IO[bytes]) -> encoding.Digest:
#         """Read the given data stream to completion, and store as a blob.

#         Return the digest of the stored blob.
#         """

#         hasher = encoding.Hasher()
#         # uuid4 is used to get around issues where a high amount of
#         # multiprocessing could cause the same machine to generate
#         # the same uuid because of a duplicate read of the current time
#         working_filename = "work-" + uuid.uuid4().hex
#         working_filepath = os.path.join(self.root, working_filename)
#         self._ensure_base_dir(working_filepath)
#         with open(working_filepath, "xb") as working_file:
#             chunk = data.read(_CHUNK_SIZE)
#             while len(chunk) > 0:
#                 hasher.update(chunk)
#                 working_file.write(chunk)
#                 chunk = data.read(_CHUNK_SIZE)

#         digest = hasher.digest()
#         final_filepath = self._build_digest_path(digest)
#         self._ensure_base_dir(final_filepath)
#         try:
#             os.rename(working_filepath, final_filepath)
#             os.chmod(final_filepath, self.blob_permissions)
#         except FileExistsError:
#             _LOGGER.debug("blob already exists", digest=digest)
#             os.remove(working_filepath)

#         return digest

#     def remove_blob(self, digest: encoding.Digest) -> None:
#         """Remove a blob from this storage."""

#         path = self._build_digest_path(digest)
#         try:
#             os.remove(path)
#         except FileNotFoundError:
#             raise graph.UnknownObjectError(digest)


class FSManifestViewer(ManifestViewer, FileDB):
    def __init__(self, root: str, payloads: PayloadStorage) -> None:

        self._storage = payloads
        ManifestViewer.__init__(self)
        FileDB.__init__(self, root)

    def render_manifest(self, manifest: tracking.Manifest) -> str:
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

        for rendered_path, entry in manifest.walk_abs(rendered_dirpath):
            if entry.kind is tracking.EntryKind.TREE:
                os.makedirs(rendered_path, exist_ok=True)
            elif entry.kind is tracking.EntryKind.MASK:
                continue
            elif entry.kind is tracking.EntryKind.BLOB:
                self._render_blob(rendered_path, entry)
            else:
                raise NotImplementedError(f"Unsupported entry kind: {entry.kind}")

        for rendered_path, entry in reversed(list(manifest.walk_abs(rendered_dirpath))):
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
        working_dirpath = os.path.join(self.__root, working_dirname)
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


class ManifestStorage(FileDB):
    def has_manifest(self, digest: encoding.Digest) -> bool:
        """Return true if the identified manifest exists in this storage."""

        path = self._build_digest_path(digest)
        return os.path.exists(path + ".manifest")

    def read_manifest(self, digest: encoding.Digest) -> tracking.Manifest:
        """Return the manifest identified by the given digest.

        Raises:
            graph.UnknownObjectError: if the manifest does not exist in this storage
        """
        path = self._build_digest_path(digest)
        try:
            with open(path + ".manifest", "rb") as f:
                return tracking.Manifest.decode(f)
        except FileNotFoundError:
            raise graph.UnknownObjectError(digest)

    def write_manifest(self, manifest: tracking.Manifest) -> None:
        """Write the given manifest into this storage."""
        path = self._build_digest_path(manifest.digest())
        self._ensure_base_dir(path)
        try:
            with open(path + ".manifest", "xb") as f:
                manifest.encode(f)
        except FileExistsError:
            pass

    def remove_manifest(self, digest: encoding.Digest) -> None:
        """Remove a manifest from this storage.

        Raises:
            graph.UnknownObjectError: if the manifest does not exist in this storage
        """
        path = self._build_digest_path(digest)
        try:
            os.remove(path + ".manifest")
        except FileNotFoundError:
            raise graph.UnknownObjectError(digest)


def _was_render_completed(render_path: str) -> bool:

    return os.path.exists(render_path + ".completed")


def _mark_render_completed(render_path: str) -> None:

    open(render_path + ".completed", "w+").close()


def _unmark_render_completed(render_path: str) -> None:

    try:
        os.remove(render_path + ".completed")
    except FileNotFoundError:
        pass


def _copy_manifest(manifest: tracking.Manifest, src_root: str, dst_root: str) -> None:
    """Copy manifest contents from one directory to another.
    """

    src_root = src_root.rstrip("/")
    dst_root = dst_root.rstrip("/")

    def get_masked_entries(dirname: str, entry_names: List[str]) -> List[str]:

        ignored = []
        manifest_path = dirname[len(src_root) :] or "/"
        for name in entry_names:
            entry_path = os.path.join(manifest_path, name)
            entry = manifest.get_path(entry_path)
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
