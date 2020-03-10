from typing import Tuple
import abc

from .. import encoding


class Object(encoding.Encodable, metaclass=abc.ABCMeta):
    """Object is the base class for all storable data types.

    Objects are identified by a hash of their contents, and
    can have any number of immediate children that they reference.
    """

    @abc.abstractmethod
    def child_objects(self) -> Tuple[encoding.Digest, ...]:
        """Identify the set of children to this object in the graph."""
        ...
