from typing import List, Union, Iterable, Dict, Optional
from collections import defaultdict
from functools import lru_cache

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


class Decision:
    def __init__(self, parent: "Decision" = None) -> None:
        self.parent = parent
        self._requests: Dict[str, List[api.Ident]] = defaultdict(list)
        self._resolved: Dict[str, api.Ident] = {}

    @lru_cache()
    def level(self) -> int:

        level = 1
        parent = self.parent
        while parent is not None:
            level += 1
            parent = parent.parent
        return level

    def set_resolved(self, pkg: api.Ident) -> None:

        self._resolved[pkg.name] = pkg

    def add_request(self, pkg: Union[str, api.Ident]) -> None:

        if not isinstance(pkg, api.Ident):
            pkg = api.parse_ident(pkg)

        # TODO: check against existing requests for impossible
        # TODO: reopen request if already complete but not satisfied

        self._requests[pkg.name].append(pkg)

    def current_packages(self) -> Dict[str, api.Ident]:

        packages = {}
        if self.parent is not None:
            packages.update(self.parent.current_packages())
        packages.update(self._resolved)

        return packages

    @lru_cache()
    def has_unresolved_requests(self) -> bool:

        return len(self.unresolved_requests()) != 0

    def next_request(self) -> Optional[api.Ident]:

        unresolved = self.unresolved_requests()
        if len(unresolved) == 0:
            return None

        return self.get_merged_request(next(iter(unresolved.keys())))

    def unresolved_requests(self) -> Dict[str, List[api.Ident]]:

        resolved = self.current_packages()
        requests = self.get_all_package_requests()

        unresolved = dict((n, r) for n, r in requests.items() if n not in resolved)
        return unresolved

    def get_all_package_requests(self) -> Dict[str, List[api.Ident]]:

        base: Dict[str, List[api.Ident]] = defaultdict(list)
        if self.parent is not None:
            base.update(self.parent.get_all_package_requests())

        for name in self._requests:
            base[name].extend(self._requests[name])

        return base

    def get_package_requests(self, name: str) -> List[api.Ident]:

        requests = []
        if self.parent is not None:
            requests.extend(self.parent.get_package_requests(name))
        requests.extend(self._requests[name])
        return requests

    def get_merged_request(self, name: str) -> Optional[api.Ident]:

        requests = self.get_package_requests(name)

        if not requests:
            return None

        if len(requests) > 1:
            raise NotImplementedError("Cannot merge requests yet")

        return requests[0]


class DecisionTree:
    def __init__(self) -> None:

        self.root = Decision()


class Solver:
    def __init__(self, options: Union[api.OptionMap, Dict[str, str]]) -> None:

        self._repos: List[storage.Repository] = []
        self._options = api.OptionMap(options.items())
        self._decisions = DecisionTree()

    def add_repository(self, repo: storage.Repository) -> None:

        self._repos.append(repo)

    def add_request(self, pkg: Union[str, api.Ident]) -> None:

        self._decisions.root.add_request(pkg)

    def solve(self) -> Dict[str, api.Ident]:

        state = self._decisions.root
        while state.has_unresolved_requests():

            try:
                state = self._solve_next_request(state)
            except UnresolvedPackageError:
                if state.parent is None:
                    raise
                state = state.parent

        return state.current_packages()

    def _solve_next_request(self, state: Decision) -> Decision:

        # FIXME: support many repos
        assert len(self._repos) <= 1, "Too many package repositories."
        assert len(self._repos), "No registered package repositories."
        repo = self._repos[0]

        request = state.next_request()
        if not request:
            raise RuntimeError("Logic error: nothing to solve in current state")
        pkg = find_best_version(repo, request, self._options)

        decision = Decision(state)
        decision.set_resolved(pkg)
        return decision


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
