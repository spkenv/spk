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
from .. import Layer

_logger = structlog.get_logger(__name__)


class LayerStorage:
    """Manages the on-disk storage of layers."""

    def __init__(self, root: str) -> None:
        """Initialize a new storage inside the given root directory."""
        self._root = os.path.abspath(root)

    def read_layer(self, digest: str) -> Layer:
        """Read a layer's information from this storage.

        Raises:
            ValueErrors: If the layer does not exist.
        """

        layer_path = os.path.join(self._root, digest)
        try:
            with open(layer_path, "r", encoding="utf-8") as f:
                data = json.load(f)
            return Layer.load_dict(data)
        except OSError as e:
            if e.errno == errno.ENOENT:
                raise ValueError("Unknown layer: " + digest)
            raise

    def remove_layer(self, digest: str) -> None:
        """Remove a layer from this storage.

        Raises:
            ValueError: If the layer does not exist.
        """

        layer_path = os.path.join(self._root, digest)
        try:
            os.remove(layer_path)
        except FileNotFoundError:
            raise ValueError("Unknown layer: " + digest)

    def list_layers(self) -> List[Layer]:
        """Return a list of the current stored layers."""

        try:
            dirs = os.listdir(self._root)
        except OSError as e:
            if e.errno == errno.ENOENT:
                dirs = []
            else:
                raise

        return [self.read_layer(d) for d in dirs]

    def commit_manifest(
        self, manifest: tracking.Manifest, env: Dict[str, str] = None
    ) -> Layer:
        """Create a layer from the file system manifest."""

        if env is None:
            env = {}

        serialized_env = sorted(f"{n}={v}" for n, v in env.items())
        layer = Layer(manifest=manifest, environ=tuple(serialized_env))

        self.write_layer(layer)
        return layer

    def write_layer(self, layer: Layer) -> None:

        digest = layer.digest
        layer_path = os.path.join(self._root, digest)
        os.makedirs(self._root, exist_ok=True)
        try:
            with open(layer_path, "x", encoding="utf-8") as f:
                json.dump(layer.dump_dict(), f)
            _logger.debug("layer created", digest=digest)
        except FileExistsError:
            _logger.debug("layer already exists", digest=digest)
