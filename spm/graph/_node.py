from typing import Iterable
import abc


class Node(metaclass=abc.ABCMeta):
    """Represents a node in the resolution graph."""

    def inputs(self) -> Iterable["Node"]:
        return []

    def outputs(self) -> Iterable["Node"]:
        return []
