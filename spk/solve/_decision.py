from typing import List, Dict, Set, Optional, Union, Iterator, Tuple, Iterable
from collections import defaultdict
from functools import lru_cache

import structlog

from .. import api, storage
from ._errors import SolverError, ConflictingRequestsError, PackageNotFoundError
from ._solution import Solution

_LOGGER = structlog.get_logger("spk.solve")


class PackageIterator(Iterator[Tuple[api.Spec, storage.Repository]]):
    """PackageIterator is a stateful cursor yielding package versions.

    The iterator yields packages from a repository which are compatible with some
    request. These are used to retain a cursor in the repo in the case of
    needing to continue with next-best option upon error or issue in the solve.
    """

    def __init__(
        self,
        repos: List[storage.Repository],
        request: api.Request,
        options: api.OptionMap,
    ) -> None:
        self._repos = repos
        self._request = request
        self._options = options
        self._versions: Optional[Iterator[str]] = None
        self._builds: Optional[Iterator[api.Ident]] = None
        self._version_map: Dict[str, storage.Repository] = {}
        self.history: Dict[api.Ident, api.Compatibility] = {}

    def _start(self) -> None:

        self._version_map = {}
        for repo in reversed(self._repos):
            versions = repo.list_package_versions(self._request.pkg.name)
            for version in versions:
                self._version_map[version] = repo

        if not len(self._version_map):
            raise PackageNotFoundError(self._request.pkg.name)

        versions = list(self._version_map.keys())
        versions.sort()
        versions.reverse()
        self._versions = iter(versions)
        self._builds = iter([])

    def clone(self) -> "PackageIterator":
        """Create a copy of this iterator, with the cursor at the same point."""

        if self._versions is None:
            self._start()

        other = PackageIterator(self._repos, self._request, self._options)
        remaining_versions = list(self._versions or [])
        remaining_builds = list(self._builds or [])
        other._versions = iter(remaining_versions)
        self._versions = iter(remaining_versions)
        self._builds = iter(remaining_builds)
        other._builds = iter(remaining_builds)
        other.history = self.history.copy()
        other._version_map = self._version_map
        return other

    def __next__(self) -> Tuple[api.Spec, storage.Repository]:

        if self._versions is None:
            self._start()

        requested_build = self._request.pkg.build
        for candidate in self._builds or []:

            if requested_build is not None:
                if requested_build != candidate.build:
                    continue
            elif candidate.build.is_source():  # type: ignore
                # TODO: support resolving/building source packages on the fly
                continue

            version_str = str(candidate.version)
            repo = self._version_map[version_str]
            spec = repo.read_spec(candidate)

            compat = self._request.is_satisfied_by(spec)
            if not compat:
                self.history[candidate] = compat
                continue

            compat = spec.build.validate_options(self._options)
            if not compat:
                self.history[candidate] = compat
                continue

            return (spec, repo)

        self._start_next_version()
        return self.__next__()

    def _start_next_version(self) -> None:

        for version_str in self._versions or []:
            version = api.parse_version(version_str)

            compat = self._request.is_version_applicable(version)
            if not compat:
                self.history[api.Ident(self._request.pkg.name, version)] = compat
                continue

            pkg = api.Ident(self._request.pkg.name, version)
            repo = self._version_map[version_str]
            try:
                spec = repo.read_spec(pkg)
            except ValueError:
                _LOGGER.error("package disappeared from repo", pkg=pkg, repo=repo)
                continue

            compat = spec.build.validate_options(self._options)
            if not compat:
                self.history[api.Ident(self._request.pkg.name, version)] = compat
                continue

            compat = self._request.pkg.version.is_satisfied_by(spec)
            if not compat:
                self.history[api.Ident(self._request.pkg.name, version)] = compat
                continue

            self._builds = iter(repo.list_package_builds(pkg))
            return

        raise StopIteration


class Decision:
    """Decision represents a change in state for the solver.

    Decisions form a tree structure. Each decision can have a single
    parent, and any number of child branches which should represent
    different possible subsequent decisions made by the solver.

    The root decision in the tree will not have a parent, and generally
    holds the set of initial requests for a resolve.

    Decisions provide the state of the resolve after this decision has been
    applied by merging the decision changes with all parents.

    Decisions usually resolve a single package request and optionally
    add additional requests (from depenencies). If a dependency
    is added which invalidates a previously resolved package, they
    can also 'reopen/unresolve' a package to be resolved again.
    If some unrecoverable issue caused the solver's not to be able to
    continue from the parent state, decision.get_error() will return
    the relevant exception.
    """

    def __init__(self, parent: "Decision" = None) -> None:
        self.parent = parent
        self.branches: List[Decision] = []
        self._requests: Dict[str, List[api.Request]] = {}
        self._resolved: Solution = Solution()
        self._unresolved: Set[str] = set()
        self._error: Optional[SolverError] = None
        self._iterators: Dict[str, PackageIterator] = {}

    def __str__(self) -> str:
        if self._error is not None:
            return f"STOP: {self._error}"
        out = ""
        if self._resolved:
            values = list(str(spec.pkg) for _, spec, _ in self._resolved.items())
            out += f"RESOLVE: {', '.join(values)} "
        if self._requests:
            values = list(str(pkg) for pkg in self._requests.values())
            out += f"REQUEST: {', '.join(values)} "
        return out

    @lru_cache()
    def level(self) -> int:
        """Return the depth of this decision in it's tree (number or parents)."""

        level = 0
        parent = self.parent
        while parent is not None:
            level += 1
            parent = parent.parent
        return level

    def set_error(self, error: SolverError) -> None:
        """Set the error on this decision, marking it as an invalid state."""

        self._error = error

    def get_error(self) -> Optional[SolverError]:
        """Get the error caused by this decision (if any)."""
        return self._error

    def set_resolved(self, spec: api.Spec, repo: storage.Repository) -> None:
        """Set the given package as resolved by this decision.

        The given spec is expected to have a fully resolved package with exact build.
        """

        self.unresolved_requests.cache_clear()
        self.get_all_unresolved_requests.cache_clear()
        request = self.get_merged_request(spec.pkg.name)  # TODO: should this be passed?
        assert request is not None, "Cannot resolve unrequested package " + str(spec)
        self.force_set_resolved(request, spec, repo)

    def force_set_resolved(
        self, request: api.Request, spec: api.Spec, repo: storage.Repository
    ) -> None:
        self._resolved.add(request, spec, repo)

    def get_resolved(self) -> Solution:
        """Get the set of packages resolved by this decision."""

        return self._resolved.clone()

    def set_unresolved(self, name: str) -> None:
        """Set the given package as unresolved by this decision.

        An unresolved package undoes any previous decision that resolves
        the package and forces the solver to resolve it again.

        This usually only makes sense if the decision introduces a new
        request which is not satisfied by the previous resolve, and will
        be called automatically in this case when Decision.add_request is called
        """

        self.unresolved_requests.cache_clear()
        self.get_all_unresolved_requests.cache_clear()
        self._unresolved.add(name)

    def get_unresolved(self) -> List[str]:
        """Get the set of packages that are unresolved by this decision."""

        return list(self._unresolved)

    def get_iterator(self, name: str) -> Optional[PackageIterator]:
        """Get the current package iterator for this state.

        The returned iterator, if not none, will iterate through any remaining
        versions of the named package that are compatible with the solver
        state represented by this decision
        """

        if name not in self._iterators:
            if self.parent is not None:
                parent_iter = self.parent.get_iterator(name)
                if parent_iter is not None:
                    self._iterators[name] = parent_iter.clone()

        return self._iterators.get(name)

    def set_iterator(self, name: str, iterator: PackageIterator) -> None:
        """Set a package iterator for this decision.

        The given iterator represents the available package verisons
        compatible with the solver state represented by this decision.
        """

        self._iterators[name] = iterator

    def add_request(self, request: Union[str, api.Ident, api.Request]) -> None:
        """Add a package request to this decision

        This request may be a new package, or a new constraint on an existing
        requested package. Either way the solver will ensure it is satisfied
        should this decision branch be deemed solvable.
        """

        if isinstance(request, api.Ident):
            request = str(request)
        if not isinstance(request, api.Request):
            request = api.Request.from_dict({"pkg": request})

        try:
            current = self.get_current_solution().get(request.pkg.name)
            if not current[1].sastisfies_request(request):
                self.set_unresolved(request.pkg.name)
        except KeyError:
            pass

        self.unresolved_requests.cache_clear()
        self.get_all_unresolved_requests.cache_clear()
        self._requests.setdefault(request.pkg.name, [])
        self._requests[request.pkg.name].append(request)

    def get_requests(self) -> Dict[str, List[api.Request]]:
        """Get the set of package requests added by this decision."""

        copy = {}
        for name, reqs in self._requests.items():
            copy[name] = list(pkg.clone() for pkg in reqs)
        return copy

    def add_branch(self) -> "Decision":
        """Add a child branch to this decision."""

        branch = Decision(parent=self)
        self.branches.append(branch)
        return branch

    def get_current_solution(self) -> Solution:
        """Get the full set of resolved packages for this decision state

        Unlike get_resolved, this includes resolved packages from all parents.
        """

        packages = Solution()
        if self.parent is not None:
            packages.update(self.parent.get_current_solution())
        packages.update(self._resolved)

        for name in self._unresolved:
            packages.remove(name)

        return packages

    def has_unresolved_requests(self) -> bool:
        """Return true if there are unsatisfied package requests in this solver state."""

        return len(self.unresolved_requests()) != 0

    def next_request(self) -> Optional[api.Request]:
        """Return the next package request to be resolved in this state."""

        unresolved = self.get_all_unresolved_requests()
        if len(unresolved) == 0:
            return None

        for name in unresolved.keys():
            req = self.get_merged_request(name)
            if req is None:
                continue
            if req.inclusion_policy is api.InclusionPolicy.Always:
                return req
        return None

    @lru_cache()
    def unresolved_requests(self) -> Dict[str, List[api.Request]]:
        """Return the set of unresolved requests for this decision."""

        resolved = self.get_current_solution()
        requests = self.get_requests()

        unresolved = {}
        for name, v in requests.items():
            request = self.get_merged_request(name)
            if request and request not in resolved:
                unresolved[name] = v

        return unresolved

    @lru_cache()
    def get_all_unresolved_requests(self) -> Dict[str, List[api.Request]]:
        """Return the complete set of unresolved requests for this solver state."""

        resolved = self.get_current_solution()
        requests = self.get_all_package_requests()

        unresolved = {}
        for name, v in requests.items():
            request = self.get_merged_request(name)
            if request and request not in resolved:
                unresolved[name] = v

        return unresolved

    def get_all_package_requests(self) -> Dict[str, List[api.Request]]:
        """Get the set of all package requests at this state, solved or not."""

        base: Dict[str, List[api.Request]] = defaultdict(list)
        if self.parent is not None:
            base.update(self.parent.get_all_package_requests())

        for name in self._requests:
            base[name].extend(self._requests[name])

        return base

    def get_package_requests(self, name: str) -> List[api.Request]:
        """Get the set of requests in this state for the named package."""

        requests = []
        if self.parent is not None:
            requests.extend(self.parent.get_package_requests(name))
        requests.extend(self._requests.get(name, []))
        return requests

    def get_merged_request(self, name: str) -> Optional[api.Request]:
        """Get a single request for the named package which satisfies all current requests for that package."""

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
    """Decision tree represents an entire set of solver decisions

    The decision tree provides convenience methods for working
    with an entire decision tree at once.
    """

    def __init__(self) -> None:

        self.root = Decision()

    def walk(self) -> Iterable[Decision]:

        to_walk = [self.root]
        while len(to_walk):
            here = to_walk.pop()
            yield here
            to_walk.extend(reversed(here.branches))

    def get_error_chain(self) -> List[SolverError]:
        """Compile the last chain of errors that were encountered.

        This is done by walking the root of the tree backwards, and once
        an decision with an error is found, walk up previous branches
        of the tree to find any previous errors that were immediately
        preceding the root one.

        The returned list of errors should provide a picture of the last
        stack unwind in the case of a failed solve. It starts with the last
        error seen and ends with it's initial cause
        """

        chain = []
        bad_decision = self.root
        while bad_decision.branches:
            last = bad_decision.branches[-1]
            err = last.get_error()
            if err is None:
                break
            chain.append(err)
            try:
                bad_decision = bad_decision.branches[-2]
            except IndexError:
                break

        return chain
