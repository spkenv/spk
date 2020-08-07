from typing import Union, Dict, Any
from dataclasses import dataclass
import base64
import binascii

SRC = "src"
EMBEDED = "embeded"


class InvalidBuildError(ValueError):
    """Denotes that an invalid build digest was given."""

    pass


@dataclass
class Build:
    """Build represents a package build identifier."""

    digest: str

    def is_source(self) -> bool:
        return self.digest == SRC

    def is_emdeded(self) -> bool:
        return self.digest == EMBEDED

    def __str__(self) -> str:
        return self.digest


def parse_build(digest: str) -> Build:

    if digest in (SRC, EMBEDED):
        return Build(digest)

    try:
        base64.b32decode(digest)
    except binascii.Error as e:
        raise InvalidBuildError(f"Invalid build digest: {e}") from None
    return Build(digest)
