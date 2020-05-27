"""SPack - an SpFS Package Manager"""

__version__ = "0.1.0"

from . import api, graph, storage, build

from ._solver import Solver, UnresolvedPackageError
from ._global import load_spec, save_spec

# promote useful front line api functions
from .api import read_spec_file
from .build import make_source_package, make_binary_package
