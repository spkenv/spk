import abc
from typing import Dict, Iterable, List, NamedTuple, Optional, Tuple

from .. import api
from ._solution import PackageSource, Solution
from ._package_iterator import PackageIterator


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
        return hash(self._state)

    @property
    def state(self) -> "State":
        return self._state

    def add_output(self, decision: "Decision") -> None:
        self._outputs.append(decision)

    def add_input(self, decision: "Decision") -> None:
        self._inputs.append(decision)

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
    options: Tuple[Tuple[str, str], ...]

    @staticmethod
    def default() -> "State":

        return State(pkg_requests=tuple(), var_requests=tuple(), options=tuple())

    def as_solution(self) -> Solution:
        raise NotImplementedError("State.current_solution")


class Decision:
    """The decision represents a choice made by the solver.

    Each decision connects one state to another in the graph.
    """

    def __init__(self, changes: Iterable["Change"] = []) -> None:

        self._changes: List[Change] = list(changes)

    def apply(self, base: State) -> State:

        state = base
        for change in self._changes:
            state = change.apply(state)
        return state


class Change(metaclass=abc.ABCMeta):
    """A single change made to a state."""

    def as_decision(self) -> Decision:
        return Decision([self])

    @abc.abstractmethod
    def apply(self, base: State) -> State:
        ...


class ResolvePackage(Change):
    def __init__(self, spec: api.Spec, source: PackageSource) -> None:
        self._spec = spec
        self._source = source

    def apply(self, base: State) -> State:
        raise NotImplementedError("ResolvePackage.apply")


class UnresolvePackage(Change):
    def apply(self, base: State) -> State:
        raise NotImplementedError("UnresolvePackage.apply")


class RequestVar(Change):
    def __init__(self, request: api.VarRequest) -> None:
        self._request = request

    def apply(self, base: State) -> State:

        return State(
            pkg_requests=base.pkg_requests,
            var_requests=base.var_requests + (self._request,),
            options=base.options,
        )


class RequestPackage(Change):
    def __init__(self, request: api.PkgRequest) -> None:
        self._request = request

    def apply(self, base: State) -> State:

        return State(
            pkg_requests=base.pkg_requests + (self._request,),
            var_requests=base.var_requests,
            options=base.options,
        )


class StepBack(Change):
    """Identifies the solver reaching an impass and needing to revert a previous decision."""

    def __init__(self, cause: Exception, to: State = None) -> None:
        self.cause = cause
        self._destination = to

    def apply(self, base: State) -> State:
        if self._destination is None:
            raise self.cause
        return self._destination


class SetOption(Change):
    def __init__(self, name: str, value: str) -> None:
        self._name = name
        self._value = value

    def apply(self, base: State) -> State:
        options = dict(base.options)
        options[self._name] = self._value
        return State(
            pkg_requests=base.pkg_requests,
            var_requests=base.var_requests,
            options=tuple(options.items()),
        )
