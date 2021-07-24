# Copyright (c) 2021 Sony Pictures Imageworks, et al.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/imageworks/spk
"""SPK - an SpFS Package Manager"""

import spkrs

from spkrs import api, solve

from . import pysolve

solve.legacy = pysolve.legacy

from . import storage, build, exec, test
from ._global import load_spec, save_spec
from ._env import current_env, NoEnvironmentError
from ._publish import Publisher

__version__ = spkrs.version()

# promote useful front line api functions
Solution = solve.Solution
Solver = solve.Solver
SolverError = solve.SolverError
from .build import (
    SourcePackageBuilder,
    BinaryPackageBuilder,
    BuildError,
    CollectionError,
)
from .storage import export_package, import_package
from .exec import build_required_packages, setup_current_runtime

__all__ = list(locals().keys())
