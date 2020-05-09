from typing import Iterator, Tuple
from ._node import Node


def walk_inputs_out(node: Node) -> Iterator[Tuple[str, Node]]:

    for child in node.inputs():
        yield ".", child
        for p, n in walk_inputs_out(child):
            yield "." + p, n


def walk_inputs_in(node: Node) -> Iterator[Tuple[str, Node]]:

    for child in node.inputs():
        for p, n in walk_inputs_in(child):
            yield "." + p, n
        yield ".", child
