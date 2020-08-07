from typing import Dict, Any, List

from ruamel import yaml

from .. import api
from .. import storage


class SolverError(Exception):
    pass


class PackageNotFoundError(SolverError, storage.PackageNotFoundError):
    pass


class UnresolvedPackageError(SolverError):
    def __init__(
        self, pkg: Any, history: Dict[api.Ident, api.Compatibility] = None
    ) -> None:

        self.message = f"Failed to resolve: {pkg}"
        self.history = history
        super(UnresolvedPackageError, self).__init__(self.message)


class ConflictingRequestsError(SolverError):
    def __init__(self, msg: str, requests: List[api.Request] = None) -> None:

        self.requests = requests
        message = f"Conflicting requests: {msg}"
        if requests is not None:
            req_list = ", ".join(yaml.safe_dump(r.to_dict()).strip() for r in requests)  # type: ignore
            message += f" - from requests: [{req_list}]"
        super(ConflictingRequestsError, self).__init__(message)
