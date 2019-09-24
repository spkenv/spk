from ._blob import BlobStorage
from ._layer import Layer, LayerStorage
from ._platform import Platform, PlatformStorage
from ._repository import Repository, Object
from ._registry import register_scheme, open_repository

# automatically registered implementations
from . import fs
