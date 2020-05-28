from typing import List, Dict, Set, Optional, Union, Iterator, Tuple, Iterable
from collections import defaultdict
from functools import lru_cache

from .. import api, storage, compat
from ._errors import SolverError, ConflictingRequestsError


class PackageIterator(Iterator[Tuple[api.Ident, api.Spec]]):
    def __init__(
        self, repo: storage.Repository, request: api.Ident, options: api.OptionMap
    ) -> None:
        self._repo = repo
        self._request = request
        self._options = options
        self._versions: Optional[Iterator[str]] = None
        self.past_versions: List[str] = []

    def _start(self) -> None:
        all_versions = self._repo.list_package_versions(self._request.name)
        versions = list(filter(self._request.version.is_satisfied_by, all_versions))
        versions.sort()
        versions.reverse()
        self._versions = iter(versions)

    def clone(self) -> "PackageIterator":

        if self._versions is None:
            self._start()

        other = PackageIterator(self._repo, self._request, self._options)
        remaining = list(self._versions)  # type: ignore
        other._versions = iter(remaining)
        self._versions = iter(remaining)
        return other

    def __next__(self) -> Tuple[api.Ident, api.Spec]:

        if self._versions is None:
            self._start()

        for version_str in self._versions:  # type: ignore
            self.past_versions.append(version_str)
            version = compat.parse_version(version_str)
            pkg = api.Ident(self._request.name, version)
            spec = self._repo.read_spec(pkg)
            options = spec.resolve_all_options(self._options)

            candidate = pkg.with_build(options.digest())
            try:
                self._repo.get_package(candidate)
            except storage.PackageNotFoundError:
                continue

            return (candidate, spec)

        raise StopIteration


class Decision:
    def __init__(self, parent: "Decision" = None) -> None:
        self.parent = parent
        self.branches: List[Decision] = []
        self._requests: Dict[str, List[api.Ident]] = defaultdict(list)
        self._resolved: Dict[str, api.Ident] = {}
        self._unresolved: Set[str] = set()
        self._error: Optional[SolverError] = None
        self._iterators: Dict[str, PackageIterator] = {}

    def __str__(self) -> str:
        if self._error is not None:
            return f"STOP: {self._error}"
        out = ""
        if self._resolved:
            values = list(str(pkg) for pkg in self._resolved.values())
            out += f"RESOLVE: {', '.join(values)} "
        if self._requests:
            values = list(str(pkg) for pkg in self._requests.values())
            out += f"REQUEST: {', '.join(values)} "
        return out

    @lru_cache()
    def level(self) -> int:

        level = 0
        parent = self.parent
        while parent is not None:
            level += 1
            parent = parent.parent
        return level

    def set_error(self, error: SolverError) -> None:

        self._error = error

    def get_error(self) -> Optional[SolverError]:
        return self._error

    def set_resolved(self, pkg: api.Ident) -> None:

        self._resolved[pkg.name] = pkg

    def get_resolved(self) -> Dict[str, api.Ident]:

        return dict((n, pkg.clone()) for n, pkg in self._resolved.items())

    def set_unresolved(self, pkg: api.Ident) -> None:

        self._unresolved.add(pkg.name)

    def get_unresolved(self) -> List[str]:

        return list(self._unresolved)

    def get_iterator(self, name: str) -> Optional[PackageIterator]:

        if name not in self._iterators:
            if self.parent is not None:
                parent_iter = self.parent.get_iterator(name)
                if parent_iter is not None:
                    self._iterators[name] = parent_iter.clone()

        return self._iterators.get(name)

    def set_iterator(self, name: str, iterator: PackageIterator) -> None:

        self._iterators[name] = iterator

    def add_request(self, pkg: Union[str, api.Ident]) -> None:

        if not isinstance(pkg, api.Ident):
            pkg = api.parse_ident(pkg)

        current = self.current_packages().get(pkg.name)
        if current is not None:
            if not pkg.version.is_satisfied_by(current.version):
                self.set_unresolved(pkg)

        self._requests[pkg.name].append(pkg)

    def get_requests(self) -> Dict[str, List[api.Ident]]:

        copy = {}
        for name, reqs in self._requests.items():
            copy[name] = list(pkg.clone() for pkg in reqs)
        return copy

    def add_branch(self) -> "Decision":

        branch = Decision(self)
        self.branches.append(branch)
        return branch

    def current_packages(self) -> Dict[str, api.Ident]:

        packages = {}
        if self.parent is not None:
            packages.update(self.parent.current_packages())
        packages.update(self._resolved)

        for name in self._unresolved:
            if name in packages:
                del packages[name]

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

        merged = requests[0].clone()
        for request in requests[1:]:
            try:
                merged.restrict(request)
            except ValueError as e:
                raise ConflictingRequestsError(str(e), requests)

        return merged


class DecisionTree:
    def __init__(self) -> None:

        self.root = Decision()

    def walk(self) -> Iterable[Decision]:

        to_walk = [self.root]
        while len(to_walk):
            here = to_walk.pop()
            yield here
            to_walk.extend(reversed(here.branches))
