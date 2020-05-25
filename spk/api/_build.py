from dataclasses import dataclass
import base64
import binascii

_SRC = "src"


@dataclass
class Build:
    """Build represents a package build identifier."""

    digest: str

    def is_source(self) -> bool:
        return self.digest == _SRC

    def __str__(self) -> str:
        return self.digest


def parse_build(digest: str) -> Build:

    if digest == _SRC:
        return Build(_SRC)

    try:
        base64.b32decode(digest)
    except binascii.Error as e:
        raise ValueError(f"Invalid build digest: {e}") from None
    return Build(digest)
