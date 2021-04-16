# Copyright (c) 2021 Sony Pictures Imageworks, et al.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/imageworks/spk

"""The previous implementation of the solver, kept around for regrassion testing."""
from .. import PackageNotFoundError
from ._errors import (
    SolverError,
    ConflictingRequestsError,
    UnresolvedPackageError,
)
from ._package_iterator import (
    PackageIterator,
    RepositoryPackageIterator,
    FilteredPackageIterator,
)
from ._decision import Decision, DecisionTree
from ._solver import Solver

__all__ = list(locals().keys())
