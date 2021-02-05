"""The previous implementation of the solver, kept around for regrassion testing."""
from ._errors import (
    SolverError,
    ConflictingRequestsError,
    UnresolvedPackageError,
    PackageNotFoundError,
)
from ._package_iterator import (
    PackageIterator,
    RepositoryPackageIterator,
    FilteredPackageIterator,
)
from ._decision import Decision, DecisionTree
from ._solver import Solver

__all__ = list(locals().keys())
