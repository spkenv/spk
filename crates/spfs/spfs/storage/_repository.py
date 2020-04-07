from typing import List, Union
import os
import stat
import io
import abc

import structlog

from .. import graph, encoding, tracking
from ._layer import LayerStorage
from ._platform import PlatformStorage
from ._blob import Blob, BlobStorage
from ._manifest import Manifest, ManifestStorage
from ._tag import TagStorage
from ._payload import PayloadStorage

_CHUNK_SIZE = 1024
_logger = structlog.get_logger("spfs.storage")


class Repository(PlatformStorage, LayerStorage, ManifestStorage, BlobStorage):
    """Repostory represents a storage location for spfs data."""

    def __init__(
        self,
        tags: TagStorage,
        object_database: graph.Database,
        payload_storage: PayloadStorage,
    ) -> None:

        self.tags = tags
        self.objects = object_database
        self.payloads = payload_storage
        super(Repository, self).__init__(object_database)

    @abc.abstractmethod
    def address(self) -> str:
        """Return the address of this repository."""
        ...

    def has_ref(self, ref: Union[str, encoding.Digest]) -> bool:

        try:
            self.read_ref(ref)
        except (graph.UnknownObjectError, graph.UnknownReferenceError):
            return False
        return True

    def read_ref(self, ref: Union[str, encoding.Digest]) -> graph.Object:
        """Read an object of unknown type by tag or digest."""
        if isinstance(ref, encoding.Digest):
            digest = ref
        else:
            try:
                digest = self.objects.resolve_full_digest(ref)
            except ValueError:
                digest = self.tags.resolve_tag(ref).target

        return self.objects.read_object(digest)

    def find_aliases(self, ref: Union[str, encoding.Digest]) -> List[str]:
        """Return the other identifiers that can be used for 'ref'."""

        aliases: List[str] = []
        digest = self.read_ref(ref).digest()
        for spec in self.tags.find_tags(digest):
            if spec not in aliases:
                aliases.append(spec)
        if ref != digest:
            aliases.append(digest.str())
            aliases.remove(str(ref))
        return aliases

    def commit_dir(self, path: str) -> tracking.Manifest:
        """Commit a local file system directory to this storage.

        This collects all files to store as blobs and maintains a
        render of the manifest for use immediately.
        """

        path = os.path.abspath(path)
        manifest = tracking.Manifest()

        _logger.info("committing files")
        for root, dirs, files in os.walk(path):

            relroot = os.path.relpath(root, path)
            manifest.mkdirs(relroot)
            for filename in files:
                # TODO: multiprocessing
                filepath = os.path.join(root, filename)
                st = os.lstat(filepath)

                if stat.S_ISLNK(st.st_mode):
                    data = os.readlink(filepath)
                    digest = self.payloads.write_payload(
                        io.BytesIO(data.encode("utf-8"))
                    )
                elif stat.S_ISREG(st.st_mode):
                    with open(filepath, "rb") as f:
                        digest = self.payloads.write_payload(f)
                else:
                    raise ValueError("Unsupported non-regular file:" + filepath)

                node = manifest.mkfile(os.path.join(relroot, filename))
                node.object = digest
                node.kind = tracking.EntryKind.BLOB
                node.mode = st.st_mode
                node.size = st.st_size

            for dirname in dirs:
                st = os.stat(os.path.join(root, dirname))
                node = manifest.mkdirs(os.path.join(relroot, dirname))
                node.object = encoding.NULL_DIGEST
                node.kind = tracking.EntryKind.TREE
                node.mode = st.st_mode
                node.size = st.st_size

        _logger.info("writing manifest")
        storable = Manifest(manifest)
        self.objects.write_object(storable)
        for _, node in manifest.walk():
            if node.kind is not tracking.EntryKind.BLOB:
                continue
            blob = Blob(node.object, node.size)
            self._db.write_object(blob)

        return manifest
