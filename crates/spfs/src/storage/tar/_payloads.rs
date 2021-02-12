from typing import Iterator, BinaryIO, Dict
import os
import io
import tarfile

import structlog

from ... import graph, encoding
from .. import PayloadStorage

_logger = structlog.get_logger("spfs.storage.tar")
_CHUNK_SIZE = 1024


class TarPayloadStorage(PayloadStorage):
    def __init__(self, tar: tarfile.TarFile) -> None:

        self._prefix = "payloads/"
        self._tar = tar
        self._payload_cache: Dict[encoding.Digest, bytes] = {}

    def iter_digests(self) -> Iterator[encoding.Digest]:

        try:
            for info in self._tar:
                if not info.name.startswith(self._prefix):
                    continue

                yield self.get_digest_from_path(info.name)
        except FileNotFoundError:
            pass

    def write_payload(self, reader: BinaryIO) -> encoding.Digest:

        payload = io.BytesIO()
        hasher = encoding.Hasher()
        size = 0

        while True:
            chunk = reader.read(_CHUNK_SIZE)
            if not chunk:
                break
            hasher.update(chunk)
            size += payload.write(chunk)

        digest = hasher.digest()
        if digest in self._payload_cache:
            return digest
        else:
            self._payload_cache[digest] = payload.getvalue()

        path = self._build_digest_path(digest)
        info = tarfile.TarInfo(path)
        info.size = size
        payload.seek(0, os.SEEK_SET)
        self._tar.addfile(info, payload)
        return digest

    def open_payload(self, digest: encoding.Digest) -> BinaryIO:

        if digest in self._payload_cache:
            payload = self._payload_cache[digest]
            return io.BytesIO(payload)

        path = self._build_digest_path(digest)
        try:
            reader = self._tar.extractfile(path)
            if reader is None:
                raise KeyError()
        except (KeyError, OSError):
            raise graph.UnknownObjectError(digest)
        return reader  # type: ignore

    def remove_payload(self, digest: encoding.Digest) -> None:

        raise NotImplementedError("Cannot remove data from a tar archive")

    def _build_digest_path(self, digest: encoding.Digest) -> str:

        digest_str = str(digest)
        return os.path.join(self._prefix, digest_str[:2], digest_str[2:])

    def get_digest_from_path(self, path: str) -> encoding.Digest:
        """Given a valid storage path, get the object digest.

        This method does not validate the path and will provide
        invalid references if given an invalid path.
        """

        path = os.path.normpath(path)
        parts = path.split(os.sep)
        return encoding.parse_digest(parts[-2] + parts[-1])

    def resolve_full_digest(self, short_digest: str) -> encoding.Digest:
        """Resolve the complete object digest from a shortened one.

        Raises:
            graph.UnknownObjectError: if the digest cannot be resolved
            graph.AmbiguousReferenceError: if the digest resolves to more than one path
        """

        options = []
        for digest in self.iter_digests():
            if digest.str().startswith(short_digest):
                options.append(digest)
        if len(options) == 0:
            raise graph.UnknownReferenceError(f"Unknown ref: {short_digest}")
        if len(options) > 1:
            raise graph.AmbiguousReferenceError(short_digest)
        return options[0]
