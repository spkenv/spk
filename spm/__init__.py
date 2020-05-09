"""The 'S' Package Manger: Convenience, clarity and speed."""

__version__ = "0.1.0"

from . import api, graph, storage

from ._planner import Planner, Plan
from ._build import build, build_variants
