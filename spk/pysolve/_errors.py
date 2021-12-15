# Copyright (c) 2021 Sony Pictures Imageworks, et al.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/imageworks/spk

from .. import storage


class SolverError(Exception):
    pass


class PackageNotFoundError(SolverError, FileNotFoundError):
    pass
