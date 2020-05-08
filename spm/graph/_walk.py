from typing import Iterator, Tuple
from ._node import Node


def walk_up(node: Node) -> Iterator[Tuple[str, Node]]:

    for child in node.inputs():
        yield ".", child
        for p, n in walk(child):
            yield "." + p, n
