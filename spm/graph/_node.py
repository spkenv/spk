from typing import Dict, TypeVar, Generic, Type, Optional
from dataclasses import dataclass
import abc

_T = TypeVar("_T")


@dataclass
class Port(Generic[_T]):

    type: Type[_T]
    owner: "Node"
    value: Optional[_T] = None
    connection: Optional["Port[_T]"] = None

    def is_connected(self) -> bool:

        return self.connection is not None

    def follow(self) -> "Node":

        if self.connection is None:
            raise RuntimeError(f"Port not connected")

        return self.connection.owner

    def connect(self, other: "Port[_T]") -> None:

        if self.type is not other.type:
            raise ValueError(f"Incompatible ports: {self.type} != {other.type}")
        self.connection = other
        other.connection = self


class Node(metaclass=abc.ABCMeta):
    """Node is a node in the graph which requires execution."""

    def __init__(self) -> None:

        self.inputs: Dict[str, Port] = {}
        self.outputs: Dict[str, Port] = {}

    def add_input_port(self, name: str, type: Type[_T]) -> Port[_T]:

        # TODO: check for squash?
        port = Port[_T](type=type, owner=self)
        self.inputs[name] = port
        return port

    def add_output_port(self, name: str, type: Type[_T]) -> Port[_T]:

        # TODO: check for squash?
        port = Port[_T](type=type, owner=self)
        self.outputs[name] = port
        return port

    def connect(self, input: str, other: "Node", output: str) -> None:

        try:
            out_port = other.outputs[output]
        except KeyError:
            raise ValueError(f"Cannot connect: no output port named '{output}'")

        self.connect_port(input, out_port)

    def connect_port(self, input: str, other: Port) -> None:

        try:
            in_port = self.inputs[input]
        except KeyError:
            raise ValueError(f"Cannot connect: no input port named '{input}'")

        in_port.connect(other)

    @abc.abstractmethod
    def run(self) -> None:
        pass
