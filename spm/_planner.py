from typing import List, Iterable

import spfs
from ruamel import yaml

from . import graph, api
from ._nodes import SourcePackageNode, BinaryPackageNode, BuildNode
from ._handle import SpFSHandle
from ._solver import Solver


class Planner:
    def __init__(self, options: api.OptionMap = None) -> None:

        self._specs: List[api.Spec] = []
        self._pkgs: List[api.Ident] = []
        self._options = options if options else api.OptionMap()

    def add_spec(self, api: api.Spec) -> None:

        self._specs.append(api)

    def add_package(self, pkg: api.Ident) -> None:

        self._pkgs.append(pkg)

    def plan(self) -> "Plan":

        plan = Plan()
        solver = Solver(self._options)

        for spec in self._specs:
            options = spec.resolve_all_options(self._options)
            plan.append(BuildNode(spec, options))

        for pkg in self._pkgs:
            solver.add_request(pkg)

        plan.extend(solver.solve())

        return plan


class Plan(graph.Node, list):
    def outputs(self) -> Iterable[graph.Node]:

        return self
