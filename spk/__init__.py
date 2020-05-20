"""The 'S' Package Manger: Convenience, clarity and speed."""

__version__ = "0.1.0"

from . import api, graph, storage

from ._solver import Solver, Env
from ._build import build, build_variants
