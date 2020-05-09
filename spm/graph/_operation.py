import abc
from ._node import Node
from ._walk import walk_inputs_in


class Operation(Node):
    """Operation is a node in the graph which requires execution."""

    @abc.abstractmethod
    def run(self) -> None:
        pass


def execute_tree(node: Node) -> None:

    for p, n in walk_inputs_in(node):

        print(f"evaluating {p}: {n}")
        if isinstance(n, Operation):
            n.run()

    if isinstance(node, Operation):
        node.run()
