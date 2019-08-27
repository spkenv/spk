from typing import List
from typing_extensions import Protocol, runtime_checkable


@runtime_checkable
class Layer(Protocol):
    """Represents one logical item in a runtime stack.

    Layers are the organizational unit that can be installed
    into a spenv environment, and which are stacked together
    to render the final environment file system.
    """

    @property
    def ref(self) -> str:
        """Return the identifying reference string for this layer."""
        ...

    @property
    def layers(self) -> List[str]:
        """Return the full set of layers represented by this one.

        If this layer does not expand or represent a larger set, the
        layer itself should be a single entry in the returned list.
        """
        ...

    @property
    def rootdir(self) -> str:
        """Return the on-disk storage location of this layer's data."""
        ...
