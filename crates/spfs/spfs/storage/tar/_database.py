from typing import Dict, Type
import io
import os
import tarfile

import structlog

from ... import graph, encoding
from .. import (
    Blob,
    Layer,
    Platform,
    Manifest,
)
from ._payloads import TarPayloadStorage

_logger = structlog.get_logger("spfs.storage.tar")
_OBJECT_HEADER = b"--SPFS--"
_OBJECT_KINDS: Dict[int, Type[graph.Object]] = {
    0: Blob,
    1: Manifest,
    2: Layer,
    3: Platform,
}


class TarDatabase(TarPayloadStorage, graph.Database):
    """An object database implementation that persists data using a local tar file."""

    def __init__(self, tar: tarfile.TarFile) -> None:

        super(TarDatabase, self).__init__(tar)
        self._prefix = "objects/"

    def read_object(self, digest: encoding.Digest) -> graph.Object:

        with self.open_payload(digest) as payload:
            reader = io.BytesIO(payload.read())

        try:
            encoding.consume_header(reader, _OBJECT_HEADER)
            kind = encoding.read_int(reader)
            if kind not in _OBJECT_KINDS:
                raise ValueError(f"Object is corrupt: unknown kind {kind} [{digest}]")
            return _OBJECT_KINDS[kind].decode(reader)
        finally:
            reader.close()

    def write_object(self, obj: graph.Object) -> None:

        for kind, cls in _OBJECT_KINDS.items():
            if isinstance(obj, cls):
                break
        else:
            raise ValueError(f"Unkown object kind, cannot store: {type(obj)}")

        filepath = self._build_digest_path(obj.digest())
        writer = io.BytesIO()
        encoding.write_header(writer, _OBJECT_HEADER)
        encoding.write_int(writer, kind)
        obj.encode(writer)
        writer.seek(0)
        info = tarfile.TarInfo(filepath)
        info.size = len(writer.getvalue())
        self._tar.addfile(info, writer)

    def remove_object(self, digest: encoding.Digest) -> None:

        self.remove_payload(digest)
