from typing import Iterable

from . import graph
from ._handle import Handle


class BinaryPackageNode(graph.Node):
    def __init__(self, handle: Handle) -> None:

        self._handle = handle

    def inputs(self) -> Iterable[graph.Node]:

        if isinstance(self._handle, graph.Node):
            return [self._handle]
        return []


class SourcePackageNode(graph.Node):

    pass
