import abc
from ._node import Node


class Operation(Node):
    """Operation is a node in the graph which requires execution."""

    @abc.abstractmethod
    def run(self) -> None:
        pass
