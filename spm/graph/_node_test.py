from typing import Dict
from ._node import Port, Node

import pytest


class DummyNode(Node):
    def run(self) -> None:
        pass


def test_port_connections() -> None:

    si = DummyNode()
    in_port = si.add_input_port("in", str)

    so = DummyNode()
    out_port = so.add_output_port("out", str)

    si.connect("in", so, "out")

    assert in_port.owner is si
    assert out_port.owner is so
    assert in_port.connection is out_port
    assert out_port.connection is in_port


def test_port_connections_types() -> None:

    si = DummyNode()
    si.add_input_port("in", str)
    io = DummyNode()
    io.add_output_port("out", int)
    with pytest.raises(ValueError):
        si.connect("in", io, "out")
