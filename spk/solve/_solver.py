from typing import Generator, Iterable, Iterator, List, Optional, Tuple, Union, Dict

from ruamel import yaml
import structlog

from .. import api, storage
from ._package_iterator import (
    RepositoryPackageIterator,
    PackageIterator,
)
from ._errors import SolverError, PackageNotFoundError
from ._solution import Solution
from . import graph, validation

_LOGGER = structlog.get_logger("spk.solve")


class SolverFailedError(SolverError):
    def __init__(self, graph: "graph.Graph") -> None:
        self.graph = graph
        super(SolverFailedError, self).__init__("Failed to resolve")


SolutionGenerator = Generator[Tuple[graph.Node, graph.Decision], None, Solution]


class SolverRuntime(Iterable[Tuple[graph.Node, graph.Decision]]):
    def __init__(self, generator: SolutionGenerator) -> None:
        self._generator = generator

    def __iter__(self) -> Iterator[Tuple[graph.Node, graph.Decision]]:
        self.solution = yield from self._generator


class Solver:
    """Solver is the main entrypoint for resolving a set of packages."""

    class OutOfOptions(SolverError):
        def __init__(
            self, request: api.PkgRequest, notes: Iterable[graph.Note] = []
        ) -> None:
            self.request = request
            self.notes = list(notes)

    def __init__(self) -> None:

        self._repos: List[storage.Repository] = []
        self._initial_state_builders: List[graph.Change] = []
        self._validators: List[validation.Validator] = validation.default_validators()
        self._last_graph = graph.Graph()

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
            request.required_compat = api.CompatRule.API
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
        self._initial_state_builders.append(
            graph.SetOptions(api.OptionMap(options.items()))
        )

    def get_initial_state(self) -> graph.State:
        state = graph.State.default()
        for change in self._initial_state_builders:
            state = change.apply(state)
        return state

    def get_last_solve_graph(self) -> graph.Graph:
        return self._last_graph

    def solve_build_environment(self, spec: api.Spec) -> Solution:
        """Adds requests for all build requirements and solves"""

        state = self.get_initial_state()

        build_options = spec.resolve_all_options(state.get_option_map())
        for option in spec.build.options:
            if not isinstance(option, api.PkgOpt):
                continue
            given = build_options.get(option.name())
            request = option.to_request(given)
            self.add_request(request)

        return self.solve()

    def solve(self, options: api.OptionMap = api.OptionMap()) -> Solution:

        runtime = self.run(options)
        for _ in runtime:
            pass
        return runtime.solution

    def run(self, options: api.OptionMap = api.OptionMap()) -> SolverRuntime:
        return SolverRuntime(self._run(options))

    def _run(self, options: api.OptionMap) -> SolutionGenerator:

        solve_graph = graph.Graph()
        self._last_graph = solve_graph

        history = []
        current_node = solve_graph.root
        decision: Optional[graph.Decision] = graph.Decision(
            self._initial_state_builders
        )
        while decision is not None and current_node is not graph.DEAD_STATE:

            yield (current_node, decision)
            try:
                next_node = solve_graph.add_branch(current_node.id, decision)
                current_node = next_node
                decision = self._step_state(current_node)
                history.append(current_node)
            except Solver.OutOfOptions as err:
                previous = history.pop().state if len(history) else graph.DEAD_STATE
                decision = graph.StepBack(
                    f"could not satisfy '{err.request.pkg}'", previous
                ).as_decision()
                decision.add_notes(err.notes)
            except Exception as err:
                previous = history.pop().state if len(history) else graph.DEAD_STATE
                decision = graph.StepBack(f"{err}", previous).as_decision()

        is_dead = current_node.state in (solve_graph.root.state, graph.DEAD_STATE)
        is_empty = len(self.get_initial_state().pkg_requests) == 0
        if is_dead and not is_empty:
            raise SolverFailedError(solve_graph)

        return current_node.state.as_solution()

    def _step_state(self, node: graph.Node) -> Optional[graph.Decision]:

        notes = []
        request = node.state.get_next_request()
        if request is None:
            return None

        decision: graph.Decision
        iterator = self._get_iterator(node, request.pkg.name)
        for pkg, builds in iterator:

            compat = request.is_version_applicable(pkg.version)
            if not compat:
                notes.append(graph.SkipPackageNote(pkg, compat))
                continue

            for spec, repo in builds:
                build_from_source = spec.pkg.is_source() and not request.pkg.is_source()
                if build_from_source:
                    if isinstance(repo, api.Spec):
                        notes.append(
                            graph.SkipPackageNote(
                                spec.pkg, "cannot build embedded source package"
                            )
                        )
                        continue
                    try:
                        spec = repo.read_spec(spec.pkg.with_build(None))
                    except storage.PackageNotFoundError:
                        notes.append(
                            graph.SkipPackageNote(
                                spec.pkg,
                                "cannot build from source, version spec not available",
                            )
                        )
                        continue

                compat = self._validate(node.state, spec)
                if not compat:
                    notes.append(graph.SkipPackageNote(spec.pkg, compat))
                    continue

                if build_from_source:
                    try:
                        build_env = self._resolve_new_build(spec, node.state)
                    except SolverError as err:
                        note = graph.SkipPackageNote(
                            spec.pkg, f"cannot resolve build env: {err}"
                        )
                        notes.append(note)
                        continue
                    decision = graph.BuildPackage(spec, repo, build_env)
                else:
                    decision = graph.ResolvePackage(spec, repo)
                decision.add_notes(notes)
                return decision

        raise Solver.OutOfOptions(request, notes)

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
        for pkg_request in state.pkg_requests:
            opts.setdefault(pkg_request.pkg.name, str(pkg_request.pkg.version))
        for var_request in state.var_requests:
            opts.setdefault(var_request.var, var_request.value)
        solver = Solver()
        solver._repos = self._repos
        solver.update_options(opts)
        return solver.solve_build_environment(spec)
