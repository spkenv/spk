from typing import List, Iterable
import collections
import itertools

from ._node import Node
from ._operation import Operation


class AggregateNode(Node, list):
    def inputs(self) -> Iterable[Node]:

        return itertools.chain(*(n.inputs() for n in self))

    def outputs(self) -> Iterable[Node]:

        return itertools.chain(*(n.outputs() for n in self))


class AggregateOperation(AggregateNode, Operation):
    def run(self) -> None:

        for node in self:
            node.run()
