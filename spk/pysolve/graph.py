# Copyright (c) 2021 Sony Pictures Imageworks, et al.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/imageworks/spk

from typing import Dict, Iterable, Iterator, List, NamedTuple, Optional, Tuple, Union
import abc
import base64
from functools import lru_cache

import structlog

from .. import api
from ._solution import PackageSource, Solution
from ._package_iterator import PackageIterator

_LOGGER = structlog.get_logger("spk.solve")


class Graph:
    """Graph contains the starting point and memory for the solver.

    The graph data structures record every state and change of state
    that the solver goes through while it resolves a set of packages.
    """

    def __init__(self) -> None:

        self._root = Node(DEAD_STATE)
        self._nodes: Dict[int, "Node"] = {self._root.id: self._root}

    @property
    def root(self) -> "Node":
        return self._root

    def walk(self) -> Iterator[Tuple["Node", "Decision"]]:

        node_outputs: Dict[int, List[Decision]] = {}

        to_process = [self.root]
        while to_process:
            node = to_process.pop(0)

            if node.id not in node_outputs:
                node_outputs[node.id] = list(node.iter_outputs())
                # the children of this node must be processed
                # before anything else (depth-first)
                for decision in reversed(list(node.iter_outputs())):
                    destination = decision.apply(node.state)
                    to_process.insert(0, self._nodes[destination.id])

            outs = node_outputs[node.id]
            if not len(outs):
                continue

            to_process.append(node)
            decision = outs.pop(0)
            yield (node, decision)

        def iter_node(node: Node) -> Iterator[Tuple[Node, Decision]]:

            while outs:
                decision = outs.pop(0)
                yield (node, decision)
                next_state = decision.apply(node.state)
                next_node = self._nodes[next_state.id]
                yield from iter_node(next_node)

        return iter_node(self._root)

    def find_deepest_errors(self) -> Optional[List[str]]:

        errors_by_level: Dict[int, List[str]] = {}
        level = 0
        for _node, decision in self.walk():
            delta = 0
            for change in decision.iter_changes():
                if isinstance(change, SetPackage):
                    delta = 1
                if isinstance(change, StepBack):
                    errors_by_level.setdefault(level, []).append(change.cause)
                    delta = -1
                    break
            level += delta

        levels_with_errors = list(errors_by_level.keys())
        if not levels_with_errors:
            return None
        highest = max(levels_with_errors)

        # we want to deduplicate but maintain order
        seen = set()
        causes = []
        for cause in errors_by_level[highest]:
            if cause not in seen:
                causes.append(cause)
                seen.add(cause)

        return causes

    def add_branch(self, source_id: int, decision: "Decision") -> "Node":

        old_node = self._nodes[source_id]
        new_state = decision.apply(old_node.state)
        new_node = Node(new_state)

        if new_node.id not in self._nodes:
            self._nodes[new_node.id] = new_node
            for name, iterator in old_node._iterators.items():
                # XXX: set_iterator also clones iterator
                new_node.set_iterator(name, iterator.clone())
        else:
            new_node = self._nodes[new_node.id]

        old_node.add_output(decision, new_node.state)
        new_node.add_input(old_node.state, decision)
        return new_node


class Node:
    """A node describes all the input and output decisions to and from a solver state."""

    def __init__(self, state: "State") -> None:

        self._inputs: Dict[int, Decision] = {}
        self._outputs: Dict[int, Decision] = {}
        self._state = state
        self._iterators: Dict[str, PackageIterator] = {}

    @lru_cache(maxsize=None)
    def __str__(self) -> str:
        encoded_id = base64.b64encode(str(self.id).encode())
        short_id = encoded_id[:6].decode()
        if self is DEAD_STATE:
            short_id = "DEAD"
        return f"Node({short_id})"

    __repr__ = __str__

    def __eq__(self, other: object) -> bool:
        if isinstance(other, Node):
            return self.id == other.id
        return False

    def __hash__(self) -> int:
        return hash(self._state)

    @property
    def id(self) -> int:
        return hash(self)

    @property
    def state(self) -> "State":
        return self._state

    def add_output(self, decision: "Decision", state: "State") -> None:
        if state.id in self._outputs:
            raise RecursionError("Branch already attempted")
        self._outputs[state.id] = decision

    def iter_outputs(self) -> Iterator["Decision"]:
        return iter(self._outputs.values())

    def add_input(self, state: "State", decision: "Decision") -> None:
        self._inputs[state.id] = decision

    def iter_inputs(self) -> Iterator["Decision"]:
        return iter(self._inputs.values())

    def get_iterator(self, package_name: str) -> Optional[PackageIterator]:
        return self._iterators.get(package_name)

    def set_iterator(self, package_name: str, iterator: PackageIterator) -> None:
        if package_name in self._iterators:
            raise ValueError("iterator already exists [INTERNAL ERROR]")
        self._iterators[package_name] = iterator.clone()


class State(NamedTuple):
    """State is an immutible point in time of the solver.

    State may represent a complete solution but usually does not.
    """

    pkg_requests: Tuple[api.PkgRequest, ...]
    var_requests: Tuple[api.VarRequest, ...]
    packages: Tuple[Tuple[api.Spec, PackageSource], ...]
    options: Tuple[Tuple[str, str], ...]
    # Cache for State.__hash__, by id.
    # Using List for interior mutability.
    # No default can be provided here because it would
    # be shared across all instances.
    hash_cache: List[int]

    @property
    def id(self) -> int:
        return hash(self)

    def __hash__(self) -> int:
        # lru_cache is not used here because it will call
        # hash(self) to determine the key.
        if self.hash_cache:
            return self.hash_cache[0]

        hashes: List[int] = []
        hashes.extend(hash(pr) for pr in self.pkg_requests)
        hashes.extend(hash(vr) for vr in self.var_requests)
        hashes.extend(hash(p) for p, _ in self.packages)
        hashes.extend(hash(o) for o in self.options)
        h = hash(tuple(hashes))
        self.hash_cache.append(h)
        return h

    @staticmethod
    def default() -> "State":

        return State(
            pkg_requests=tuple(),
            var_requests=tuple(),
            options=tuple(),
            packages=tuple(),
            hash_cache=[],
        )

    @lru_cache(maxsize=None)
    def get_option_map(self) -> api.OptionMap:

        return api.OptionMap(dict(self.options))

    def get_next_request(self) -> Optional[api.PkgRequest]:
        # tests reveal this method is not safe to cache.

        packages = set(spec.pkg.name for spec, _ in self.packages)
        for request in self.pkg_requests:
            if request.pkg.name in packages:
                continue
            if request.inclusion_policy == "IfAlreadyPresent":
                continue
            break
        else:
            return None

        return self.get_merged_request(request.pkg.name)

    def get_merged_request(self, name: str) -> api.PkgRequest:
        # tests reveal this method is not safe to cache.

        merged: Optional[api.PkgRequest] = None
        requests = iter(self.pkg_requests)
        while merged is None:
            try:
                request = next(requests)
            except StopIteration:
                raise KeyError(f"No requests for '{name}' [INTERNAL ERROR]")
            if request.pkg.name != name:
                continue
            merged = request.copy()
            break

        for request in requests:
            if request.pkg.name != merged.pkg.name:
                continue
            merged.restrict(request)

        return merged

    @lru_cache(maxsize=None)
    def get_current_resolve(self, name: str) -> api.Spec:

        for spec, _ in self.packages:
            if spec.pkg.name == name:
                return spec
        raise KeyError(f"Has not been resolved: '{name}'")

    def as_solution(self) -> Solution:
        solution = Solution(api.OptionMap(**dict(self.options)))
        for spec, source in self.packages:
            req = self.get_merged_request(spec.pkg.name)
            solution.add(req, spec, source)

        return solution


DEAD_STATE = State.default()


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
    def __init__(self, spec: api.Spec, source: PackageSource) -> None:

        self.spec = spec
        self.source = source
        super(ResolvePackage, self).__init__(self._generate_changes())

    def _generate_changes(self) -> Iterator["Change"]:

        yield SetPackage(self.spec, self.source)

        # installation options are not relevant for source packages
        if self.spec.pkg.is_source():
            return

        for req in self.spec.install.requirements:
            if isinstance(req, api.PkgRequest):
                yield RequestPackage(req)
            elif isinstance(req, api.VarRequest):
                yield RequestVar(req)
            else:
                _LOGGER.warning(f"unhandled install requirement {type(req)}")

        for embedded in self.spec.install.embedded:
            yield RequestPackage(api.PkgRequest.from_ident(embedded.pkg))
            yield SetPackage(embedded, self.spec)

        opts = api.OptionMap()
        opts[self.spec.pkg.name] = api.render_compat(
            self.spec.compat, self.spec.pkg.version
        )
        for opt in self.spec.build.options:
            value = opt.get_value()
            if value:
                name = opt.namespaced_name(self.spec.pkg.name)
                opts[name] = value
        if opts:
            yield SetOptions(opts)


class BuildPackage(Decision):
    def __init__(
        self, spec: api.Spec, source: PackageSource, build_env: Solution
    ) -> None:

        self.spec = spec
        self.source = source
        self.env = build_env
        super(BuildPackage, self).__init__(self._generate_changes())

    def _generate_changes(self) -> Iterator["Change"]:

        specs = list(s.spec for s in self.env.items())
        options = self.env.options()
        spec = self.spec.copy()
        spec.update_spec_for_build(options, specs)

        yield SetPackageBuild(spec, self.spec)
        for req in spec.install.requirements:
            if isinstance(req, api.PkgRequest):
                req = req.copy()
                yield RequestPackage(req)
            elif isinstance(req, api.VarRequest):
                yield RequestVar(req)
            else:
                _LOGGER.warning(f"unhandled install requirement {type(req)}")

        opts = api.OptionMap()
        opts[self.spec.pkg.name] = api.render_compat(
            self.spec.compat, self.spec.pkg.version
        )
        for opt in spec.build.options:
            name = opt.namespaced_name(spec.pkg.name)
            value = opt.get_value()
            if value:
                opts[name] = value
        if opts:
            yield SetOptions(opts)


class Change(metaclass=abc.ABCMeta):
    """A single change made to a state."""

    def as_decision(self) -> Decision:
        return Decision([self])

    @abc.abstractmethod
    def apply(self, base: State) -> State:
        ...


class RequestVar(Change):
    def __init__(self, request: api.VarRequest) -> None:
        self.request = request

    def apply(self, base: State) -> State:

        options = filter(lambda o: o[0] != self.request.var, base.options)
        return State(
            pkg_requests=base.pkg_requests,
            var_requests=base.var_requests + (self.request,),
            options=tuple(options) + ((self.request.var, self.request.value),),
            packages=base.packages,
            hash_cache=[],
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
            hash_cache=[],
        )

    def __repr__(self) -> str:
        return f"RequestPackage(request={self.request})"


class StepBack(Change):
    """Identifies the solver reaching an impass and needing to revert a previous decision."""

    def __init__(self, cause: str, to: State = DEAD_STATE) -> None:
        self.cause = cause
        self.destination = to

    def apply(self, base: State) -> State:
        return self.destination


class SetPackage(Change):
    def __init__(self, spec: api.Spec, source: PackageSource) -> None:
        self.spec = spec
        self.source = source

    def apply(self, base: State) -> State:
        return State(
            pkg_requests=base.pkg_requests,
            var_requests=base.var_requests,
            packages=base.packages + ((self.spec, self.source),),
            options=base.options,
            hash_cache=[],
        )


class SetPackageBuild(SetPackage):
    """Sets a package in the resolve, denoting is as a new build."""

    def __init__(self, spec: api.Spec, source: api.Spec) -> None:
        super().__init__(spec, source)


class SetOptions(Change):
    def __init__(self, options: api.OptionMap) -> None:
        self.options = options.copy()

    def apply(self, base: State) -> State:

        options = dict(base.options)
        for k, v in self.options.items():
            if v == "" and k in options:
                continue
            options[k] = v

        return State(
            pkg_requests=base.pkg_requests,
            var_requests=base.var_requests,
            options=tuple(options.items()),
            packages=base.packages,
            hash_cache=[],
        )

    def __repr__(self) -> str:
        return f"SetOptions(options={self.options})"


class Note(metaclass=abc.ABCMeta):
    @abc.abstractmethod
    def __str__(self) -> str:
        ...


class SkipPackageNote(Note):
    def __init__(self, pkg: api.Ident, reason: Union[str, api.Compatibility]) -> None:
        self.pkg = pkg
        self.reason = str(reason)

    def __str__(self) -> str:
        return f"Skipped {self.pkg} - {self.reason}"
