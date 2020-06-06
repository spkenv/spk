from typing import List, Union, Iterable, Dict, Optional, Tuple, Any, Iterator, Set
from collections import defaultdict
from functools import lru_cache

import structlog
import spfs

from .. import api, storage
from ._decision import Decision, PackageIterator, DecisionTree
from ._errors import SolverError, UnresolvedPackageError, ConflictingRequestsError

_LOGGER = structlog.get_logger("spk.solve")


class Solver:
    """Solver is the main entrypoint for resolving a set of packages."""

    def __init__(self, options: Union[api.OptionMap, Dict[str, str]]) -> None:

        self._repos: List[storage.Repository] = []
        self._options = api.OptionMap(options.items())
        self.decision_tree = DecisionTree()
        self._running = False
        self._complete = False

    def add_repository(self, repo: storage.Repository) -> None:
        """Add a repository where the solver can get packages."""

        self._repos.append(repo)

    def add_request(self, pkg: Union[str, api.Ident, api.Request]) -> None:
        """Add a package request to this solver.

        Raises:
            RuntimeError: if the solver has already completed
        """

        if self._complete:
            raise RuntimeError("Solver has already been executed")
        self.decision_tree.root.add_request(pkg)

    def solve(self) -> Dict[str, api.Spec]:
        """Solve the current set of package requests into a complete environment.

        Raises:
            RuntimeError: if the solver has already completed
        """

        if self._complete:
            raise RuntimeError("Solver has already been executed")
        self._running = True

        state = self.decision_tree.root
        while state.has_unresolved_requests():

            try:
                state = self._solve_next_request(state)
            except SolverError:
                if state.parent is None:
                    raise UnresolvedPackageError(state.next_request().to_dict())  # type: ignore
                state = state.parent

        self._running = False
        self._complete = True
        return state.get_current_packages()

    def _solve_next_request(self, state: Decision) -> Decision:

        decision = state.add_branch()
        try:

            request = state.next_request()
            if not request:
                raise RuntimeError("Logic error: nothing to solve in current state")

            iterator = state.get_iterator(request.pkg.name)
            if iterator is None:
                iterator = self._make_iterator(request)
                state.set_iterator(request.pkg.name, iterator)

            pkg, spec = next(iterator)
            spec.pkg = pkg
            decision.set_resolved(spec)
            for dep in spec.depends:
                decision.add_request(dep)

        except StopIteration:
            err = UnresolvedPackageError(request.to_dict(), versions=iterator.past_versions)  # type: ignore
            decision.set_error(err)
            raise err
        except SolverError as e:
            decision.set_error(e)
            raise

        return decision

    def _make_iterator(self, request: api.Request) -> PackageIterator:
        # FIXME: support many repos
        assert len(self._repos) <= 1, "Too many package repositories."
        assert len(self._repos), "No registered package repositories."
        repo = self._repos[0]

        return PackageIterator(repo, request, self._options)
