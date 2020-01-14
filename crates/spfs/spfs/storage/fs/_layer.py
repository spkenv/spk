from typing import NamedTuple, Tuple, List, Dict, IO, Optional, Iterable
import os
import enum
import uuid
import stat
import json
import errno
import shutil
import hashlib
import subprocess

import structlog

from ... import tracking
from .. import Layer, UnknownObjectError
from ._digest_store import DigestStorage

_logger = structlog.get_logger(__name__)


class LayerStorage(DigestStorage):
    """Manages the on-disk storage of layers."""

    def __init__(self, root: str) -> None:
        """Initialize a new storage inside the given root directory."""

        super(LayerStorage, self).__init__(root)

    def read_layer(self, digest: str) -> Layer:
        """Read a layer's information from this storage.

        Raises:
            ValueErrors: If the layer does not exist.
        """

        try:
            layer_path = self.resolve_full_digest_path(digest)
            with open(layer_path, "r", encoding="utf-8") as f:
                data = json.load(f)
            return Layer.load_dict(data)
        except UnknownObjectError:
            raise UnknownObjectError("Unknown layer: " + digest)
        except OSError as e:
            if e.errno == errno.ENOENT:
                raise UnknownObjectError("Unknown layer: " + digest)
            raise

    def remove_layer(self, digest: str) -> None:
        """Remove a layer from this storage.

        Raises:
            ValueError: If the layer does not exist.
        """

        try:
            layer_path = self.resolve_full_digest_path(digest)
            os.remove(layer_path)
        except (FileNotFoundError, UnknownObjectError):
            raise UnknownObjectError("Unknown layer: " + digest)

    def list_layers(self) -> List[Layer]:
        """Return a list of the current stored layers."""

        return list(self.iter_layers())

    def iter_layers(self) -> Iterable[Layer]:
        """Step through each of the current stored layers."""

        for digest in self.iter_digests():
            yield self.read_layer(digest)

    def commit_manifest(self, manifest: tracking.Manifest) -> Layer:
        """Create a layer from the file system manifest."""

        layer = Layer(manifest=manifest)

        self.write_layer(layer)
        return layer

    def write_layer(self, layer: Layer) -> None:

        digest = layer.digest
        self.ensure_digest_base_dir(digest)
        layer_path = self.build_digest_path(digest)
        try:
            with open(layer_path, "x", encoding="utf-8") as f:
                json.dump(layer.dump_dict(), f)
            _logger.debug("layer created", digest=digest)
        except FileExistsError:
            _logger.debug("layer already exists", digest=digest)
