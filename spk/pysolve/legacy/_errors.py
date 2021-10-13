# Copyright (c) 2021 Sony Pictures Imageworks, et al.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/imageworks/spk

from typing import Dict, Any, Sequence

from ... import api
from .. import SolverError


class UnresolvedPackageError(SolverError):
    def __init__(
        self, pkg: Any, history: Dict[api.Ident, api.Compatibility] = None
    ) -> None:

        self.message = f"Failed to resolve: {pkg}"
        self.history = history
        super(UnresolvedPackageError, self).__init__(self.message)


class ConflictingRequestsError(SolverError):
    def __init__(self, msg: str, requests: Sequence[api.Request] = None) -> None:

        self.requests = requests
        message = f"Conflicting requests: {msg}"
        super(ConflictingRequestsError, self).__init__(message)
