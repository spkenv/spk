import abc

from . import graph, api


class BinaryPackageHandle:
    def __init__(self, spec: api.Spec, ref: str) -> None:

        self.spec = spec
        self.ref = ref

    def __str__(self) -> str:

        return f"BuildNode( pkg={self.spec.pkg}, ref={self.ref} )"


class SourcePackageHandle:
    def __init__(self, spec: api.Spec, ref: str) -> None:

        self.spec = spec
        self.ref = ref

    def __str__(self) -> str:

        return f"SourcePackageHandle( pkg={self.spec.pkg}, ref={self.ref} )"
