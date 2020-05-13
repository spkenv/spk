from typing import Iterator, Tuple
from ._node import Node


def walk_inputs_out(node: Node) -> Iterator[Tuple[str, Node]]:

    for name, port in node.inputs.items():
        if port.is_connected():
            node = port.follow()
            yield name, node
            for p, n in walk_inputs_out(node):
                yield "name/" + p, n


def walk_inputs_in(node: Node) -> Iterator[Tuple[str, Node]]:

    for name, port in node.inputs.items():
        if port.is_connected():
            node = port.follow()
            for p, n in walk_inputs_out(node):
                yield "name/" + p, n
            yield name, node
