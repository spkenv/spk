from typing import List, Union, Iterable, Dict, Optional, Tuple, Any, Iterator, Set
from collections import defaultdict
from functools import lru_cache
from itertools import chain

import structlog
import spfs

from .. import api, storage
from ._decision import Decision, PackageIterator, DecisionTree
from ._errors import SolverError, UnresolvedPackageError, ConflictingRequestsError
from ._solution import Solution

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

    def solve(self) -> Solution:
        """Solve the current set of package requests into a complete environment.

        Raises:
            RuntimeError: if the solver has already completed
        """

        if self._complete:
            raise RuntimeError("Solver has already been executed")

        assert len(self._repos), "No configured package repositories."
        self._running = True

        state = self.decision_tree.root
        request = state.next_request()
        while request is not None:

            try:
                state = self._solve_request(state, request)
            except SolverError:
                if state.parent is None:
                    stack = self.decision_tree.get_error_chain()
                    raise stack[-1] from None
                state = state.parent

            request = state.next_request()

        self._running = False
        self._complete = True
        return state.get_current_solution()

    def _solve_request(self, state: Decision, request: api.Request) -> Decision:

        decision = state.add_branch()
        try:

            iterator = state.get_iterator(request.pkg.name)
            if iterator is None:
                iterator = self._make_iterator(request)
                state.set_iterator(request.pkg.name, iterator)

            spec, repo = next(iterator)
            decision.set_resolved(spec, repo)
            for dep in spec.depends:
                decision.add_request(dep)

        except StopIteration:
            err = UnresolvedPackageError(request.to_dict(), history=iterator.history)
            decision.set_error(err)
            raise err from None
        except SolverError as e:
            decision.set_error(e)
            raise

        return decision

    def _make_iterator(self, request: api.Request) -> PackageIterator:

        return PackageIterator(self._repos, request, self._options)
