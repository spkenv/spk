from typing import Dict, Any, List

from .. import api
from .. import storage


class SolverError(Exception):
    pass


class PackageNotFoundError(SolverError, storage.PackageNotFoundError):
    pass


class UnresolvedPackageError(SolverError):
    def __init__(self, pkg: Any, history: Dict[str, str] = None) -> None:

        self.message = f"Failed to resolve: {pkg}"
        self.history = history
        super(UnresolvedPackageError, self).__init__(self.message)


class ConflictingRequestsError(SolverError):
    def __init__(self, msg: str, requests: List[api.Request] = None) -> None:

        message = f"Conflicting requests: {msg}"
        if requests is not None:
            req_list = ", ".join(str(r) for r in requests)
            message += f" - from requests: [{req_list}]"
        super(ConflictingRequestsError, self).__init__(message)
