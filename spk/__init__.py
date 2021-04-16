# Copyright (c) 2021 Sony Pictures Imageworks, et al.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/imageworks/spk

"""SPack - an SpFS Package Manager"""

__version__ = "0.28.0"

from . import api, storage, solve, build, exec, test
from ._global import load_spec, save_spec
from ._env import current_env, NoEnvironmentError
from ._publish import Publisher

# promote useful front line api functions
from .solve import (
    Solver,
    Solution,
    SolverError,
)
from .api import read_spec_file
from .build import (
    SourcePackageBuilder,
    BinaryPackageBuilder,
    BuildError,
    CollectionError,
)
from .storage import export_package, import_package
from .exec import build_required_packages, setup_current_runtime

__all__ = list(locals().keys())
