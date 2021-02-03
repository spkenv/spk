import abc

from .. import api
from . import graph


class Validator(metaclass=abc.ABCMeta):
    @abc.abstractmethod
    def validate(self, state: graph.State, spec: api.Spec) -> api.Compatibility:
        """Check if the given package is appropriate for the provided state."""
        ...


class BinaryOnly(Validator):
    """Enforces the resolution of binary packages only, denying new builds from source."""

    def validate(self, state: graph.State, spec: api.Spec) -> api.Compatibility:
        if spec.pkg.build is not None:
            return api.COMPATIBLE
        else:
            return api.Compatibility("Only binary packages are allowed")
