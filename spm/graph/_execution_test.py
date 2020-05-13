from ._execution import execute_tree
from ._node import Node


class SimpleOp(Node):
    def __init__(self) -> None:
        super(SimpleOp, self).__init__()
        self.complete = False

    def run(self) -> None:
        self.complete = True


def test_execute_tree_single() -> None:

    op = SimpleOp()
    execute_tree(op)

    assert op.complete


def test_execute_tree_simple_inputs() -> None:

    leaf = SimpleOp()
    leaf.add_output_port("out", int)

    middle = SimpleOp()
    middle.add_input_port("in", int)
    middle.add_output_port("out", int)

    end = SimpleOp()
    end.add_input_port("in", int)

    middle.connect("in", leaf, "out")
    end.connect("in", middle, "out")

    execute_tree(end)
    assert leaf.complete
    assert middle.complete
    assert end.complete
