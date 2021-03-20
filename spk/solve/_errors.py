from .. import storage


class SolverError(Exception):
    pass


class PackageNotFoundError(SolverError, storage.PackageNotFoundError):
    pass
