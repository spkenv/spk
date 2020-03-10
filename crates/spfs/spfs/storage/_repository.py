from typing import List, Tuple, Union, BinaryIO
from typing_extensions import Protocol, runtime_checkable
import os
import io
import stat

import structlog

from .. import graph, encoding, tracking
from ._layer import LayerStorage
from ._platform import PlatformStorage
from ._blob import BlobStorage
from ._manifest import ManifestStorage
from ._tag import TagStorage
from ._payload import PayloadStorage
from ._errors import AmbiguousReferenceError, UnknownReferenceError

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

    def address(self) -> str:
        """Return the address of this repository."""
        ...
        # TODO: fill this in?

    def get_shortened_digest(self, digest: encoding.Digest) -> str:
        """Return the shortened version of the given digest."""

        # TODO: it's possible for this size to become ambiguous
        # and we should be ensuring that this is the shortest
        # non-ambiguous reference that is available.
        return digest.str()[:10]

    def has_ref(self, ref: Union[str, encoding.Digest]) -> bool:

        try:
            self.read_ref(ref)
        except (graph.UnknownObjectError, UnknownReferenceError):
            return False
        return True

    def read_ref(self, ref: Union[str, encoding.Digest]) -> graph.Object:
        """Read an object of unknown type by tag or digest."""
        if isinstance(ref, encoding.Digest):
            digest = ref
        else:
            try:
                digest = encoding.parse_digest(ref)
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
        builder = tracking.ManifestBuilder(path)

        _logger.info("committing files")
        for root, dirs, files in os.walk(path):

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

                # TODO: store the blob entry with the size
                builder.add_entry(
                    os.path.join(root, filepath),
                    tracking.Entry(
                        object=digest,
                        kind=tracking.EntryKind.BLOB,
                        mode=st.st_mode,
                        name=filename,
                    ),
                )

            for dirname in dirs:
                dirpath = os.path.join(root, dirname)
                st = os.stat(dirpath)
                builder.add_entry(
                    dirpath,
                    tracking.Entry(
                        object=encoding.NULL_DIGEST,
                        kind=tracking.EntryKind.TREE,
                        mode=st.st_mode,
                        name=dirname,
                    ),
                )

        _logger.info("finalizing manifest")
        manifest = builder.finalize()
        self.objects.write_object(manifest)

        return manifest
