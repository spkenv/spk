from typing import Iterable

from . import graph, api
from ._handle import Handle, SpFSHandle


class BinaryPackageNode(graph.Node):
    def __init__(self, handle: Handle) -> None:

        self._handle = handle

    def __str__(self) -> str:

        return f"BuildNode( handle={self._handle.url()} )"

    def inputs(self) -> Iterable[graph.Node]:

        if isinstance(self._handle, graph.Node):
            return [self._handle]
        return []


class SourcePackageNode(graph.Node):
    def __init__(self, spec: api.Spec) -> None:

        self._spec = spec

    def __str__(self) -> str:

        return f"SourcePackageNode( pkg={self._spec.pkg} )"

    def inputs(self) -> Iterable[graph.Node]:

        return [FetchNode(self._spec)]


class FetchNode(graph.Operation):
    """Fetch node is responsible for fetching and packaging sources."""

    def __init__(self, spec: api.Spec) -> None:

        self._spec = spec

    def __str__(self) -> str:

        return f"FetchNode( pkg={self._spec.pkg} )"

    def run(self) -> None:

        # TODO: actually create the source package
        print("FETCHING SOURCES FOR:", self._spec.pkg)


class BuildNode(graph.Operation):
    def __init__(self, spec: api.Spec, options: api.OptionMap) -> None:

        self._spec = spec
        self._options = options.copy()

    def inputs(self) -> Iterable[graph.Node]:

        # FIXME: this needs to resolve build dependencies...
        return [SourcePackageNode(self._spec)]

    def outputs(self) -> Iterable[graph.Node]:
        tag = f"{self._spec.pkg.name}/{self._spec.pkg.version}/{self._options.digest()}"
        return [BinaryPackageNode(SpFSHandle(self._spec, tag))]

    def run(self) -> None:

        # FIXME: this needs to run the build if needed
        raise NotImplementedError("SpecBuilder.run")
