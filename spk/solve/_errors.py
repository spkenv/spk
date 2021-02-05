from typing import Dict, Any, List, Sequence

from ruamel import yaml

from .. import api
from .. import storage


class SolverError(Exception):
    pass


class PackageNotFoundError(SolverError, storage.PackageNotFoundError):
    pass
