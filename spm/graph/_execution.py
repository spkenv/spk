from ._node import Node


def execute_tree(op: Node) -> None:

    for name, input_port in op.inputs.items():

        input_node = input_port.follow()
        execute_tree(input_node)

    op.run()
