from typing import NamedTuple, Tuple, List, Dict, IO, Optional, Iterable
import os
import enum
import uuid
import stat
import errno
import shutil
import hashlib
import subprocess

import structlog
import simplejson

from ... import tracking
from .. import Layer as LayerConfig

_logger = structlog.get_logger(__name__)


class Layer:
    """Layers represent a logical collection of software artifacts.

    Layers are considered completely immutable, and are
    uniquely identifyable by the computed hash of all
    relevant file and metadata.
    """

    # TODO: does meta need to be a dir?
    _metadir = "meta"
    _configfile = "config.json"
    dirs = _metadir

    def __init__(self, root: str) -> None:
        """Create a new instance to represent the layer data at 'root'."""
        self._root = os.path.abspath(root)
        self._config: Optional[LayerConfig] = None

    def __repr__(self) -> str:
        return f"Layer({self._root})"

    @property
    def digest(self) -> str:
        """Return the identifying reference of this layer.

        This is usually the hash string of all relevant file and metadata.
        """
        return self.config.digest

    @property
    def layers(self) -> List[str]:
        return [self.digest]

    @property
    def rootdir(self) -> str:
        """Return the root directory where this layer is stored."""
        return self._root

    @property
    def configfile(self) -> str:
        """Return the path to this layer's config file."""
        return os.path.join(self._root, self._configfile)

    @property
    def metadir(self) -> str:
        """Return the directory in which the metadata is stored."""
        return os.path.join(self._root, self._metadir)

    @property
    def config(self) -> LayerConfig:
        """Return this layer's configuration data."""

        if self._config is None:
            return self._read_config()
        return self._config

    def _write_config(self) -> None:

        with open(self.configfile, "w+", encoding="utf-8") as f:
            self.config.dump_json(f)

    def _read_config(self) -> LayerConfig:

        try:
            with open(self.configfile, "r", encoding="utf-8") as f:
                self._config = LayerConfig.load_json(f)
        except OSError as e:
            if e.errno == errno.ENOENT:
                self._config = LayerConfig()
                self._write_config()
            else:
                raise
        return self._config

    def read_manifest(self) -> tracking.Manifest:
        """Read the cached file manifest of this layer."""
        reader = tracking.ManifestReader(self.metadir)
        return reader.read()


def _ensure_layer(path: str) -> Layer:

    os.makedirs(path, exist_ok=True, mode=0o777)
    for subdir in Layer.dirs:
        os.makedirs(os.path.join(path, subdir), exist_ok=True, mode=0o777)
    return Layer(path)


class LayerStorage:
    """Manages the on-disk storage of layers."""

    def __init__(self, root: str) -> None:
        """Initialize a new storage inside the given root directory."""
        self._root = os.path.abspath(root)

    def read_layer(self, digest: str) -> Layer:
        """Read layer information from this storage.

        Args:
            digest (str): The identifier for the layer to read.

        Raises:
            ValueError: If the layer does not exist.

        Returns:
            Layer: The layer data.
        """

        layer_path = os.path.join(self._root, digest)
        if not os.path.exists(layer_path):
            raise ValueError(f"Unknown layer: {digest}")
        return Layer(layer_path)

    def _ensure_layer(self, digest: str) -> Layer:

        layer_dir = os.path.join(self._root, digest)
        return _ensure_layer(layer_dir)

    def remove_layer(self, digest: str) -> None:
        """Remove a layer from this storage.

        Args:
            digest (str): The identifier for the layer to remove.

        Raises:
            ValueError: If the layer does not exist.
        """

        dirname = os.path.join(self._root, digest)
        try:
            shutil.rmtree(dirname)
        except OSError as e:
            if e.errno == errno.ENOENT:
                raise ValueError("Unknown layer: " + digest)
            raise

    def list_layers(self) -> List[Layer]:
        """List all stored layers.

        Returns:
            List[Layer]: The stored layers.
        """

        try:
            dirs = os.listdir(self._root)
        except OSError as e:
            if e.errno == errno.ENOENT:
                dirs = []
            else:
                raise

        return [Layer(os.path.join(self._root, d)) for d in dirs]

    def commit_manifest(
        self, manifest: tracking.Manifest, env: Dict[str, str] = None
    ) -> Layer:
        """Create a layer from the file system manifest."""

        if env is None:
            env = {}

        tree = manifest.get_path("./")
        assert tree is not None, "manifest must have entry for root dir"

        tmp_layer = self._ensure_layer("work-" + uuid.uuid1().hex)

        _logger.info("writing file manifest")
        writer = tracking.ManifestWriter(tmp_layer.metadir)
        writer.rewrite(manifest)

        _logger.info("storing layer configuation")
        config = LayerConfig(
            manifest=tree.digest,
            environ=tuple(sorted(f"{n}={v}" for n, v in env.items())),
        )
        tmp_layer._config = config
        tmp_layer._write_config()

        _logger.info("finalizing layer")
        new_root = os.path.join(self._root, config.digest)
        try:
            os.rename(tmp_layer._root, new_root)
            _logger.debug("layer created", digest=config.digest)
        except FileExistsError:
            _logger.debug("layer already exists", digest=config.digest)
            shutil.rmtree(tmp_layer.rootdir)

        return self.read_layer(config.digest)
