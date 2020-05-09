from typing import List, Iterable

import spfs
from ruamel import yaml

from . import graph, api
from ._handle import SpFSHandle


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

        for spec in self._specs:
            options = spec.resolve_all_options(self._options)
            plan.append(SpecBuilder(spec, options))
        for pkg in self._pkgs:
            plan.append(resolve_package(pkg, self._options))

        return plan


class Plan(graph.Node, list):
    def outputs(self) -> Iterable[graph.Node]:

        return self


class SpecBuilder(graph.Operation):
    def __init__(self, spec: api.Spec, options: api.OptionMap) -> None:

        self._spec = spec
        self._options = options.copy()

    def inputs(self) -> Iterable[graph.Node]:

        # FIXME: this needs to resolve build dependencies...
        return [SourcePackage(self._spec.pkg)]

    def outputs(self) -> Iterable[graph.Node]:

        tag = f"spm/pkg/{self._spec.pkg.name}/{self._spec.pkg.version}/{self._options.digest()}"
        return [SpFSHandle(self._spec, tag)]

    def run(self) -> None:

        # FIXME: this needs to run the build if needed
        raise NotImplementedError("SpecBuilder.run")
