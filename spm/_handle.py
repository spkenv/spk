import abc

from . import graph, api


class Handle(metaclass=abc.ABCMeta):
    """Handle represents a defined package with attached file system data."""

    @abc.abstractmethod
    def url(self) -> str:
        pass

    @abc.abstractmethod
    def spec(self) -> api.Spec:
        pass


class SpFSHandle(Handle):
    def __init__(self, spec: api.Spec, ref: str) -> None:

        self._spec = spec
        self._ref = ref

    def __str__(self) -> str:
        return f"{self.spec().pkg} | {self.url()}"

    def spec(self) -> api.Spec:
        return self._spec

    def url(self) -> str:
        return f"spfs:/{self._ref}"
