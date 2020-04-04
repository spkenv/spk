from typing import Dict, Type
import io
import os

import structlog

from ... import graph, encoding, tracking
from .. import (
    Blob,
    Layer,
    Platform,
)
from ._payloads import FSPayloadStorage

_logger = structlog.get_logger("spfs.storage.fs")
_OBJECT_HEADER = b"--SPFS--"
_OBJECT_KINDS: Dict[int, Type[graph.Object]] = {
    0: Blob,
    1: tracking.Manifest,
    2: Layer,
    3: Platform,
}


class FSDatabase(FSPayloadStorage, graph.Database):
    """An object database implementation that persists data using the local file system."""

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
        self._ensure_base_dir(filepath)
        try:
            with open(filepath, "xb") as writer:
                encoding.write_header(writer, _OBJECT_HEADER)
                encoding.write_int(writer, kind)
                obj.encode(writer)
        except FileExistsError:
            return

    def remove_object(self, digest: encoding.Digest) -> None:

        self.remove_payload(digest)


def makedirs_with_perms(dirname: str, perms: int = 0o777) -> None:
    """Recursively create the given directory with the appropriate permissions."""

    dirnames = os.path.normpath(dirname).split(os.sep)
    for i in range(2, len(dirnames) + 1):
        dirname = os.path.join("/", *dirnames[0:i])

        try:
            # even though checking existance first is not
            # needed, it is required to trigger the automounter
            # in cases when the desired path is in that location
            if not os.path.exists(dirname):
                os.mkdir(dirname, mode=0o777)
        except FileExistsError:
            continue

        try:
            os.chmod(dirname, perms)
        except PermissionError:
            # not fatal, so it's worth allowing things to continue
            # even though it could cause permission issues later on
            pass
