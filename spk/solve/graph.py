import abc
from typing import Dict, Iterable, Iterator, List, NamedTuple, Optional, Tuple

import structlog

from .. import api
from ._errors import SolverError
from ._solution import PackageSource, Solution
from ._package_iterator import PackageIterator

_LOGGER = structlog.get_logger("spk.solve")


class Graph:
    """Graph contains the starting point and memory for the solver.

    The graph data structures record every state and change of state
    that the solver goes through while it resolves a set of packages.
    """

    def __init__(self, initial_state: "State") -> None:

        self._root = Node(initial_state)
        self._nodes: Dict[int, "Node"] = {self._root.id: self._root}

    @property
    def root(self) -> "Node":
        return self._root

    def walk(self) -> Iterator[Tuple["Node", "Decision"]]:

        node_outputs: Dict[int, List[Decision]] = {}

        def iter_node(node: Node) -> Iterator[Tuple[Node, Decision]]:

            outs = node_outputs.setdefault(node.id, list(node.iter_outputs()))
            while outs:
                decision = outs.pop(0)
                yield (node, decision)
                next_state = decision.apply(node.state)
                next_node = self._nodes[next_state.id]
                yield from iter_node(next_node)

        return iter_node(self._root)

    def add_branch(self, source_id: int, decision: "Decision") -> "Node":

        old_node = self._nodes[source_id]
        new_state = decision.apply(old_node.state)
        new_node = Node(new_state)
        new_node = self._nodes.setdefault(new_node.id, new_node)
        old_node.add_output(decision)
        new_node.add_input(decision)
        return new_node


class Node:
    """A node describes all the input and output decisions to and from a solver state."""

    def __init__(self, state: "State") -> None:

        self._inputs: List[Decision] = []
        self._outputs: List[Decision] = []
        self._state = state
        self._iterators: Dict[str, PackageIterator] = {}

    @property
    def id(self) -> int:
        return self._state.id

    @property
    def state(self) -> "State":
        return self._state

    def add_output(self, decision: "Decision") -> None:
        self._outputs.append(decision)

    def iter_outputs(self) -> Iterator["Decision"]:
        return iter(self._outputs)

    def add_input(self, decision: "Decision") -> None:
        self._inputs.append(decision)

    def iter_inputs(self) -> Iterator["Decision"]:
        return iter(self._inputs)

    def get_iterator(self, package_name: str) -> Optional[PackageIterator]:
        return self._iterators.get(package_name)

    def set_iterator(self, package_name: str, iterator: PackageIterator) -> None:
        self._iterators[package_name] = iterator


class State(NamedTuple):
    """State is an immutible point in time of the solver.

    State may represent a complete solution but usually does not.
    """

    pkg_requests: Tuple[api.PkgRequest, ...]
    var_requests: Tuple[api.VarRequest, ...]
    packages: Tuple[api.Spec, ...]
    options: Tuple[Tuple[str, str], ...]

    @property
    def id(self) -> int:
        return hash(self)

    @staticmethod
    def default() -> "State":

        return State(
            pkg_requests=tuple(),
            var_requests=tuple(),
            options=tuple(),
            packages=tuple(),
        )

    def get_next_request(self) -> Optional[api.PkgRequest]:

        packages = set(s.pkg.name for s in self.packages)
        next_request: Optional[api.PkgRequest] = None
        requests = iter(self.pkg_requests)
        while next_request is None:
            try:
                request = next(requests)
            except StopIteration:
                return None
            if request.pkg.name in packages:
                continue
            next_request = request.clone()

        for request in requests:
            if request.pkg.name != next_request.pkg.name:
                continue
            request.restrict(request)

        return next_request

    def get_merged_request(self, name: str) -> api.PkgRequest:

        merged: Optional[api.PkgRequest] = None
        requests = iter(self.pkg_requests)
        while merged is None:
            try:
                request = next(requests)
            except StopIteration:
                raise KeyError(f"No requests for '{name}'")
            if request.pkg.name != name:
                continue
            merged = request.clone()

        for request in requests:
            if request.pkg.name != merged.pkg.name:
                continue
            request.restrict(request)

        return merged

    def get_current_resolve(self, name: str) -> api.Spec:

        for spec in self.packages:
            if spec.pkg.name == name:
                return spec
        raise KeyError(f"Has not been resolved: '{name}'")

    def as_solution(self) -> Solution:
        solution = Solution(api.OptionMap(self.options))
        for spec in self.packages:
            req = self.get_merged_request(spec.pkg.name)
            solution.add(req, spec, None)  # type: ignore

        return solution


class Decision:
    """The decision represents a choice made by the solver.

    Each decision connects one state to another in the graph.
    """

    def __init__(
        self, changes: Iterable["Change"], notes: Iterable["Note"] = []
    ) -> None:

        self._changes = list(changes)
        self._notes = list(notes)

    def add_notes(self, notes: Iterable["Note"]) -> None:
        self._notes.extend(notes)

    def iter_notes(self) -> Iterator["Note"]:
        return iter(self._notes)

    def iter_changes(self) -> Iterator["Change"]:
        return iter(self._changes)

    def apply(self, base: State) -> State:

        state = base
        for change in self.iter_changes():
            state = change.apply(state)
        return state


class ResolvePackage(Decision):
    def __init__(self, spec: api.Spec, source: PackageSource,) -> None:

        self.spec = spec
        self.source = source
        super(ResolvePackage, self).__init__(self._generate_changes())

    def _generate_changes(self) -> Iterator["Change"]:

        yield SetPackage(self.spec, self.source)
        for req in self.spec.install.requirements:
            if isinstance(req, api.PkgRequest):
                yield RequestPackage(req)
            elif isinstance(req, api.VarRequest):
                yield RequestVar(req)
            else:
                _LOGGER.warning(f"unhandled install requirement {type(req)}")

        for opt in self.spec.build.options:
            # FIXME: downgrade to package var options if var option
            yield SetOption(opt.name(), opt.get_value())


class Change(metaclass=abc.ABCMeta):
    """A single change made to a state."""

    def as_decision(self) -> Decision:
        return Decision([self])

    @abc.abstractmethod
    def apply(self, base: State) -> State:
        ...


class UnresolvePackage(Change):
    def __init__(self, pkg: api.Ident, cause: str) -> None:
        self.pkg = pkg
        self.cause = cause

    def apply(self, base: State) -> State:
        packages = filter(lambda spec: spec.pkg.name != self.pkg.name, base.packages)
        return State(
            pkg_requests=base.pkg_requests,
            var_requests=base.var_requests,
            packages=tuple(packages),
            options=base.options,
        )


class RequestVar(Change):
    def __init__(self, request: api.VarRequest) -> None:
        self.request = request

    def apply(self, base: State) -> State:

        return State(
            pkg_requests=base.pkg_requests,
            var_requests=base.var_requests + (self.request,),
            options=base.options,
            packages=base.packages,
        )


class RequestPackage(Change):
    def __init__(self, request: api.PkgRequest) -> None:
        self.request = request

    def apply(self, base: State) -> State:

        return State(
            pkg_requests=base.pkg_requests + (self.request,),
            var_requests=base.var_requests,
            options=base.options,
            packages=base.packages,
        )


class StepBack(Change):
    """Identifies the solver reaching an impass and needing to revert a previous decision."""

    def __init__(self, cause: str, to: State = None) -> None:
        self.cause = cause
        self.destination = to

    def apply(self, base: State) -> State:
        if self.destination is None:
            raise SolverError(self.cause)
        return self.destination


class SetPackage(Change):
    def __init__(self, spec: api.Spec, source: PackageSource) -> None:
        self.spec = spec
        self.source = source

    def apply(self, base: State) -> State:
        return State(
            pkg_requests=base.pkg_requests,
            var_requests=base.var_requests,
            packages=base.packages + (self.spec,),
            options=base.options,
        )


class SetOption(Change):
    def __init__(self, name: str, value: str) -> None:
        self.name = name
        self.value = value

    def apply(self, base: State) -> State:
        options = dict(base.options)
        options[self.name] = self.value
        return State(
            pkg_requests=base.pkg_requests,
            var_requests=base.var_requests,
            options=tuple(options.items()),
            packages=base.packages,
        )


class Note(metaclass=abc.ABCMeta):
    @abc.abstractmethod
    def __str__(self) -> str:
        ...


class SkipPackageNote(Note):
    def __init__(self, pkg: api.Ident, reason: str) -> None:
        self.pkg = pkg
        self.reason = reason

    def __str__(self) -> str:
        return f"Skipped {self.pkg} - {self.reason}"
