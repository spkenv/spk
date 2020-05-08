from typing import List

from typing import Iterable
import spfs

from ruamel import yaml

from . import graph
from ._spec import Spec
from ._ident import Ident
from ._option_map import OptionMap
from ._handle import SpFSHandle


class Planner:
    def __init__(self, options: OptionMap = None) -> None:

        self._specs: List[Spec] = []
        self._pkgs: List[Ident] = []
        self._options = options if options else OptionMap()

    def add_spec(self, spec: Spec) -> None:

        self._specs.append(spec)

    def add_package(self, pkg: Ident) -> None:

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


def resolve_package(pkg: Ident, options: OptionMap) -> graph.Node:

    all_versions = sorted(spfs.ls_tags(f"spm/pkg/{pkg.name}"))
    versions = list(filter(pkg.version.is_satisfied_by, all_versions))
    versions.sort()

    if not versions:
        raise ValueError(
            f"unsatisfiable request: {pkg} from versions [{', '.join(all_versions)}]"
        )

    for version in reversed(versions):

        original_spec = f"spm/meta/{pkg.name}/{version}"
        repo = spfs.get_config().get_repository()
        blob = repo.tags.resolve_tag(original_spec)
        with repo.payloads.open_payload(blob) as spec_file:
            spec_data = yaml.safe_load(spec_file)
            spec = Spec.from_dict(spec_data)

        options = spec.resolve_all_options(options)
        tag = f"spm/pkg/{pkg.name}/{version}/{options.digest()}"

        if repo.tags.has_tag(tag):
            return SpFSHandle(spec, tag)
        else:
            return SpecBuilder(spec, options)


class SpecBuilder(graph.Operation):
    def __init__(self, spec: Spec, options: OptionMap) -> None:

        self._spec = spec
        self._options = options.copy()

    def inputs(self) -> Iterable[graph.Node]:

        # FIXME: this needs to resolve build dependencies...
        return []

    def outputs(self) -> Iterable[graph.Node]:

        tag = f"spm/pkg/{self._spec.pkg.name}/{self._spec.pkg.version}/{self._options.digest()}"
        return [SpFSHandle(self._spec, tag)]

    def run(self) -> None:

        # FIXME: this needs to run the build if needed
        pass
