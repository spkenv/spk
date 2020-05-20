from ._node import Node, PortNotConnectedError

import structlog

_LOGGER = structlog.get_logger("spk")


class NoInputError(PortNotConnectedError):
    def __init__(self, *path: str) -> None:

        self.path = path + ("???",)

    def __str__(self) -> str:

        return "Missing Input: " + " -> ".join(self.path)


def execute_tree(op: Node) -> None:

    path = []

    def _exec(op: Node) -> None:
        for name, input_port in op.inputs.items():

            path.append(name)
            try:
                input_node = input_port.follow()
            except PortNotConnectedError as e:
                if input_port.value is not None:
                    _LOGGER.info("using cached result: ")
                else:
                    raise NoInputError(*path)
            else:
                _exec(input_node)
            path.pop()

        op.run()

    _exec(op)
