import abc
from typing import Dict, Iterable, List, NamedTuple, Tuple

from .. import api


class Graph:
    """Graph contains the starting point and memory for the solver.

    The graph data structures record every state and change of state
    that the solver goes through while it resolves a set of packages.
    """

    def __init__(self, initial_state: "State") -> None:

        self._initial_state = initial_state
        self._nodes = Dict[int, "Node"]


class Node:
    """A node describes all the input and output decisions to and from a solver state."""

    def __init__(self, state: "State") -> None:

        self._inputs: List[Decision] = []
        self._outputs: List[Decision] = []
        self._state = state


class State(NamedTuple):
    """State is an immutible point in time of the solver.

    State may represent a complete solution but usually does not.
    """

    pkg_requests: Tuple[api.PkgRequest, ...]
    var_requests: Tuple[api.VarRequest, ...]

    @staticmethod
    def default() -> "State":

        return State(pkg_requests=tuple(), var_requests=tuple(),)


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

    @abc.abstractmethod
    def apply(self, base: State) -> State:
        ...


class ResolvePackage(Change):
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
        )


class RequestPackage(Change):
    def __init__(self, request: api.PkgRequest) -> None:
        self._request = request

    def apply(self, base: State) -> State:

        return State(
            pkg_requests=base.pkg_requests + (self._request,),
            var_requests=base.var_requests,
        )
