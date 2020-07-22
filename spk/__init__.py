"""SPack - an SpFS Package Manager"""

__version__ = "0.9.0"

from . import api, storage, solve, build, exec
from ._global import load_spec, save_spec
from ._env import load_env, current_env, NoEnvironmentError

# promote useful front line api functions
from .solve import (
    Solver,
    UnresolvedPackageError,
    ConflictingRequestsError,
    SolverError,
    DecisionTree,
    Decision,
)
from .api import read_spec_file
from .build import (
    SourcePackageBuilder,
    BinaryPackageBuilder,
    BuildError,
    CollectionError,
)
from .storage import export_package, import_package
from .exec import setup_current_runtime, create_runtime
