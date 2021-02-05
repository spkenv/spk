from typing import Iterable, List, Optional, Union, Dict

from ruamel import yaml
import structlog

from .. import api, storage
from ._package_iterator import (
    RepositoryPackageIterator,
    FilteredPackageIterator,
    PackageIterator,
)
from ._decision import Decision, DecisionTree
from ._errors import SolverError, UnresolvedPackageError, ConflictingRequestsError
from ._solution import Solution
from . import graph, validation

_LOGGER = structlog.get_logger("spk.solve")


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
            given = build_options.get(option.name())
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
                elif not spec.pkg.build.is_source():
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

        spec = spec.clone()
        spec.update_for_build(opts, list(s for _, s, _ in solution.items()))
        for request in spec.install.requirements:
            try:
                state.add_request(request)
            except ConflictingRequestsError as err:
                return api.Compatibility(str(err))

        return api.COMPATIBLE


class GraphSolver:
    class OutOfOptions(SolverError):
        def __init__(self, notes: Iterable[graph.Note] = []) -> None:
            self.notes = list(notes)

    def __init__(self) -> None:

        self._repos: List[storage.Repository] = []
        self._initial_state_builders: List[graph.Change] = []
        self._validators: List[validation.Validator] = validation.default_validators()
        self._last_graph = graph.Graph(graph.State.default())

    def reset(self) -> None:

        self._repos.clear()
        self._initial_state_builders.clear()
        self._validators = validation.default_validators()

    def add_repository(self, repo: storage.Repository) -> None:
        """Add a repository where the solver can get packages."""

        self._repos.append(repo)

    def add_request(
        self, request: Union[str, api.Ident, api.Request, graph.Change]
    ) -> None:
        """Add a request to this solver."""

        if isinstance(request, api.Ident):
            request = str(request)

        if isinstance(request, str):
            request = api.PkgRequest.from_dict({"pkg": request})
            request = graph.RequestPackage(request)

        if isinstance(request, api.PkgRequest):
            request = graph.RequestPackage(request)
        elif isinstance(request, api.VarRequest):
            request = graph.RequestVar(request)

        if not isinstance(request, graph.Change):
            raise NotImplementedError(f"unhandled request type: {type(request)}")

        self._initial_state_builders.append(request)

    def set_binary_only(self, binary_only: bool) -> None:
        """If true, only solve pre-built binary packages.

        When false, the solver may return packages where the build is not set.
        These packages are known to have a source package available, and the requested
        options are valid for a new build of that source package.
        These packages are not actually built as part of the solver process but their
        build environments are fully resolved and dependencies included
        """
        self._validators = list(
            filter(lambda v: not isinstance(v, validation.BinaryOnly), self._validators)
        )
        if binary_only:
            self._validators.insert(0, validation.BinaryOnly())

    def update_options(self, options: Union[Dict[str, str], api.OptionMap]) -> None:
        for name, value in options.items():
            self._initial_state_builders.append(graph.SetOption(name, value))

    def get_last_solve_graph(self) -> graph.Graph:
        return self._last_graph

    def solve_build_environment(self, spec: api.Spec) -> Solution:
        """Adds requests for all build requirements and solves"""

        state = graph.State.default()
        for change in self._initial_state_builders:
            state = change.apply(state)

        build_options = spec.resolve_all_options(state.get_option_map())
        for option in spec.build.options:
            if not isinstance(option, api.PkgOpt):
                continue
            given = build_options.get(option.name())
            request = option.to_request(given)
            self.add_request(request)

        return self.solve()

    def solve(self, options: api.OptionMap = api.OptionMap()) -> Solution:

        initial_state = graph.State.default()
        solve_graph = graph.Graph(initial_state)
        self._last_graph = solve_graph

        history = []
        current_node = solve_graph.root
        decision: Optional[graph.Decision] = graph.Decision(
            self._initial_state_builders
        )
        while decision is not None:
            next_node = solve_graph.add_branch(current_node.id, decision)
            current_node = next_node
            try:
                decision = self._step_state(solve_graph, current_node)
                history.append(current_node)
            except GraphSolver.OutOfOptions as err:
                previous = history.pop().state if len(history) else None
                decision = graph.StepBack("no more versions", previous).as_decision()
                decision.add_notes(err.notes)

        return current_node.state.as_solution()

    def _step_state(
        self, solve_graph: graph.Graph, node: graph.Node
    ) -> Optional[graph.Decision]:

        notes = []
        request = node.state.get_next_request()
        if request is None:
            return None

        iterator = self._get_iterator(node, request.pkg.name)
        for spec, repo in iterator:
            print(spec.pkg, spec.deprecated)
            build_from_source = spec.pkg.is_source() and not request.pkg.is_source()
            if build_from_source:
                try:
                    spec = repo.read_spec(spec.pkg.with_build(None))
                except storage.PackageNotFoundError:
                    graph.SkipPackageNote(
                        spec.pkg, "cannot build from source, version spec not available"
                    )

            compat = self._validate(node.state, spec)
            if not compat:
                notes.append(graph.SkipPackageNote(spec.pkg, compat))
                continue

            if build_from_source:
                try:
                    build_env = self._resolve_new_build(spec, node.state)
                except SolverError as err:
                    note = graph.SkipPackageNote(
                        spec.pkg, f"failed to resolve build env: {err}"
                    )
                    notes.append(note)
                    continue
                decision = graph.BuildPackage(spec, repo, build_env)
            else:
                decision = graph.ResolvePackage(spec, repo)
            decision.add_notes(notes)
            return decision

        raise GraphSolver.OutOfOptions(notes)

    def _validate(self, node: graph.State, spec: api.Spec) -> api.Compatibility:

        for validator in self._validators:
            compat = validator.validate(node, spec)
            if not compat:
                return compat

        return api.COMPATIBLE

    def _get_iterator(self, node: graph.Node, package_name: str) -> PackageIterator:

        iterator = node.get_iterator(package_name)
        if iterator is None:
            iterator = self._make_iterator(package_name)
            node.set_iterator(package_name, iterator)

        return iterator

    def _make_iterator(self, package_name: str) -> RepositoryPackageIterator:

        assert len(self._repos), "No configured package repositories."
        return RepositoryPackageIterator(package_name, self._repos)

    def _resolve_new_build(self, spec: api.Spec, state: graph.State) -> Solution:

        opts = state.get_option_map()
        solver = GraphSolver()
        solver._repos = self._repos
        solver.update_options(opts)
        return solver.solve_build_environment(spec)
