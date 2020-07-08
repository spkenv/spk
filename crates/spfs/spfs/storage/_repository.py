from typing import List, Union, BinaryIO
import os
import stat
import io
import abc

import structlog

from .. import graph, encoding, tracking, runtime
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

    def concurrent(self) -> bool:
        """Return true if this repository supports concurrent access."""
        return False

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
                digest = self.tags.resolve_tag(ref).target
            except ValueError:
                digest = self.objects.resolve_full_digest(ref)

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

    def commit_blob(self, reader: BinaryIO) -> encoding.Digest:

        digest = self.payloads.write_payload(reader)
        blob = Blob(digest, reader.tell())
        self.objects.write_object(blob)
        return digest

    def commit_dir(self, path: str) -> tracking.Manifest:
        """Commit a local file system directory to this storage.

        This collects all files to store as blobs and maintains a
        render of the manifest for use immediately.
        """

        path = os.path.abspath(path)
        builder = tracking.ManifestBuilder()
        builder.blob_hasher = self.commit_blob

        _logger.info("committing files")
        manifest = builder.compute_manifest(path)

        _logger.info("writing manifest")
        storable = Manifest(manifest)
        self.objects.write_object(storable)
        for _, node in manifest.walk():
            if node.kind is not tracking.EntryKind.BLOB:
                continue
            blob = Blob(node.object, node.size)
            self._db.write_object(blob)

        return manifest
