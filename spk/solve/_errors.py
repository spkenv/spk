from typing import List, Any

from .. import api


class SolverError(Exception):
    pass


class UnresolvedPackageError(SolverError):
    def __init__(self, pkg: Any, versions: List[str] = None) -> None:

        message = f"Failed to resolve: {pkg}"
        if versions is not None:
            version_list = ", ".join(versions)
            message += f" - from versions: [{version_list}]"
        super(UnresolvedPackageError, self).__init__(message)


class ConflictingRequestsError(SolverError):
    def __init__(self, msg: str, requests: List[api.Ident] = None) -> None:

        message = f"Conflicting requests: {msg}"
        if requests is not None:
            req_list = ", ".join(str(r) for r in requests)
            message += f" - from requests: [{req_list}]"
        super(ConflictingRequestsError, self).__init__(message)
