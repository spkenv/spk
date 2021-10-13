# Copyright (c) 2021 Sony Pictures Imageworks, et al.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/imageworks/spk

from typing import List, Union, Dict

from ruamel import yaml
import structlog

from ... import api, storage
from .. import Solution
from ._package_iterator import (
    RepositoryPackageIterator,
    FilteredPackageIterator,
)
from ._decision import Decision, DecisionTree
from ._errors import SolverError, UnresolvedPackageError, ConflictingRequestsError

_LOGGER = structlog.get_logger("spk.solve.legacy")


class Solver:
    """Solver is the main entrypoint for resolving a set of packages."""

    def __init__(self, options: Union[api.OptionMap, Dict[str, str]]) -> None:

        self._repos: List[storage.Repository] = []
        self.decision_tree = DecisionTree()
        self.decision_tree.root.update_options(options)
        self._running = False
        self._complete = False
        self._binary_only = False

    def reset(self) -> None:

        self._repos = []
        self.decision_tree = DecisionTree()
        self._running = False
        self._complete = False
        self._binary_only = False

    def set_binary_only(self, binary_only: bool) -> None:
        """If true, only solve pre-built binary packages.

        When false, the solver may return packages where the build is not set.
        These packages are known to have a source package available, and the requested
        options are valid for a new build of that source package.
        These packages are not actually built as part of the solver process but their
        build environments are fully resolved and dependencies included
        """
        self._binary_only = binary_only

    def update_options(self, options: Union[Dict[str, str], api.OptionMap]) -> None:

        self.decision_tree.root.update_options(options)

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

    def solve_build_environment(self, spec: api.Spec) -> Solution:
        """Adds requests for all build """

        build_options = spec.resolve_all_options(self.decision_tree.root.get_options())
        for option in spec.build.options:
            if not isinstance(option, api.PkgOpt):
                continue
            given = build_options.get(option.pkg)
            request = option.to_request(given)
            self.add_request(request)

        return self.solve()

    def solve(self) -> Solution:
        """Solve the current set of package requests into a complete environment.

        Raises:
            RuntimeError: if the solver has already completed
        """

        if self._complete:
            raise RuntimeError("Solver has already been executed")

        self._running = True

        state = self.decision_tree.root
        request = state.next_request()
        while request is not None:

            if request.pin:
                _LOGGER.warning(
                    "Solving for unpinned request, this is probably not what you want to be happening!",
                    request=request,
                )

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
        solution = state.get_current_solution()
        return solution

    def _solve_request(self, state: Decision, request: api.PkgRequest) -> Decision:

        decision = state.add_branch()
        iterator = state.get_iterator(request.pkg.name)
        if iterator is None:
            iterator = self._make_iterator(decision, request)
            state.set_iterator(request.pkg.name, iterator)

        try:

            while True:
                spec, repo = next(iterator)
                if spec.pkg.build is None:
                    if self._binary_only:
                        compat = api.Compatibility("Only binary packages are allowed")
                    else:
                        compat = self._resolve_new_build(spec, state)
                    if not compat:
                        iterator.add_history(spec.pkg, compat)
                        continue
                elif not spec.pkg.build == api.SRC:
                    try:
                        with decision.transaction() as t:
                            for dep in spec.install.requirements:
                                t.add_request(dep)
                    except ConflictingRequestsError as e:
                        iterator.add_history(spec.pkg, api.Compatibility(str(e)))
                        continue
                break

            decision.set_resolved(spec, repo)

        except StopIteration:
            err = UnresolvedPackageError(
                yaml.safe_dump(request.to_dict()).strip(),  # type: ignore
                history=iterator.get_history(),
            )
            decision.set_error(err)
            raise err from None
        except SolverError as e:
            decision.set_error(e)
            raise

        return decision

    def _make_iterator(
        self, state: Decision, request: api.PkgRequest
    ) -> FilteredPackageIterator:

        assert len(self._repos), "No configured package repositories."
        return FilteredPackageIterator(
            RepositoryPackageIterator(request.pkg.name, self._repos),
            request,
            state.get_options(),
        )

    def _resolve_new_build(self, spec: api.Spec, state: Decision) -> api.Compatibility:

        opts = state.get_options()
        solver = Solver(opts)
        for repo in self._repos:
            solver.add_repository(repo)

        try:
            solution = solver.solve_build_environment(spec)
        except SolverError as err:
            return api.Compatibility(f"Failed to resolve build env: {err}")

        spec = spec.copy()
        spec.update_spec_for_build(opts, list(s for _, s, _ in solution.items()))
        for request in spec.install.requirements:
            try:
                state.add_request(request)
            except ConflictingRequestsError as err:
                return api.Compatibility(str(err))

        return api.COMPATIBLE
