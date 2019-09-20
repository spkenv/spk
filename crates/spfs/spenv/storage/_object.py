from typing_extensions import Protocol, runtime_checkable


@runtime_checkable
class Object(Protocol):
    @property
    def ref(self) -> str:
        ...
