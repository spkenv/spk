from typing import List, Optional, Union
import os
import uuid
import hashlib
import errno

from ._config import Config
from ._layer import LayerStorage, Layer
from ._runtime import RuntimeStorage, Runtime
from ._repository import Ref, Repository, ensure_repository

config = Config()


def configured_repository() -> Repository:

    return ensure_repository(config.repo_root)
