from typing import Iterator, Tuple
from ._node import Node


def walk_inputs_out(node: Node) -> Iterator[Tuple[str, Node]]:

    for name, port in node.inputs.items():
        yield name, port
        if port.is_connected():
            for p, n in walk_inputs_out(port.follow()):
                yield "name/" + p, n


def walk_inputs_in(node: Node) -> Iterator[Tuple[str, Node]]:

    for name, port in node.inputs.items():
        for p, n in walk_inputs_in(port):
            yield "." + p, n
        yield ".", port
