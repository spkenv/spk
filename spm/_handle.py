import abc

from ._spec import Spec


class Handle(metaclass=abc.ABCMeta):
    """Handle represents a defined package with attached file system data."""

    @abc.abstractmethod
    def url(self) -> str:
        pass

    @abc.abstractmethod
    def spec(self) -> Spec:
        pass


class SpFSHandle(Handle):
    def __init__(self, spec: Spec, ref: str) -> None:

        self._spec = spec
        self._ref = ref

    def __str__(self) -> str:
        return f"{self.spec().pkg} | {self.url()}"

    def spec(self) -> Spec:
        return self._spec

    def url(self) -> str:
        return f"spfs:/{self._ref}"
