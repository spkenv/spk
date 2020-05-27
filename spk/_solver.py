from typing import List, Union, Iterable, Dict

import structlog
import spfs

from . import graph, api, storage, compat
from ._handle import BinaryPackageHandle, SourcePackageHandle
from ._nodes import BuildNode, FetchNode

_LOGGER = structlog.get_logger("spk")


class UnresolvedPackageError(RuntimeError):
    def __init__(self, pkg: str, versions: List[str] = None) -> None:

        message = f"{pkg}"
        if versions is not None:
            version_list = "\n".join(versions)
            message += f" - from versions: [{version_list}]"
        super(UnresolvedPackageError, self).__init__(message)


class Solver:
    def __init__(self, options: Union[api.OptionMap, Dict[str, str]]) -> None:

        self._repos: List[storage.Repository] = []
        self._options = api.OptionMap(options.items())
        self._requests: List[api.Ident] = []
        self._specs: List[api.Spec] = []

    def add_repository(self, repo: storage.Repository) -> None:

        self._repos.append(repo)

    def add_request(self, pkg: Union[str, api.Ident]) -> None:

        if not isinstance(pkg, api.Ident):
            pkg = api.parse_ident(pkg)

        self._requests.append(pkg)

    def add_spec(self, spec: api.Spec) -> None:

        self._specs.append(spec)

    def solve(self) -> Dict[str, api.Ident]:

        # FIXME: support many repos
        assert len(self._repos) <= 1, "Too many package repositories."
        assert len(self._repos), "No registered package repositories."
        repo = self._repos[0]

        packages: Dict[str, api.Ident] = {}
        for request in self._requests:

            pkg = find_best_version(repo, request, self._options)
            packages[pkg.name] = pkg

        return packages


def find_best_version(
    repo: storage.Repository, request: api.Ident, options: api.OptionMap
) -> api.Ident:

    all_versions = repo.list_package_versions(request.name)
    all_versions.sort()
    versions = list(filter(request.version.is_satisfied_by, all_versions))
    versions.sort()

    for version_str in reversed(versions):

        version = compat.parse_version(version_str)
        pkg = api.Ident(request.name, version)
        spec = repo.read_spec(pkg)
        options = spec.resolve_all_options(options)

        candidate = pkg.with_build(options.digest())
        try:
            repo.get_package(candidate)
        except storage.PackageNotFoundError:
            _LOGGER.debug(f"build does not exist: {candidate}", **options)
            continue

        return candidate

    else:
        raise UnresolvedPackageError(str(request), versions=all_versions)
