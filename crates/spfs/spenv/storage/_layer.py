from typing_extensions import Protocol


class Layer(Protocol):
    """Represents one logical item in an environment stack.

    Layers are the organizational unit that can be installed
    into a spenv environment, and which are stacked together
    to render the final environment file system.
    """

    rootdir: str
